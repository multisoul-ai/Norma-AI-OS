import AVFoundation
import SwiftUI
import NormaCore

/// One App-owned media session, independent of the browsed computer and navigation identity.
/// GPT/WebRTC and Doubao/PCM transports migrated from companion, sharing one call lifecycle.
@MainActor final class VoiceCall: ObservableObject {
    static let serviceOrigin = VoiceAuthorization.serviceOrigin
    enum State: String { case idle = "随时可以聊聊", connecting = "正在接通", connected = "通话中", reconnecting = "正在重新连接", paused = "通话已暂停", failed = "未能接通" }
    struct Target {
        let machine: PairedMachine
        let project: ProjectSnapshot
    }
    @Published var state: State = .idle
    @Published var status = "聊想法、安排任务，或问问现在的进展。"
    @Published var subtitle = ""
    @Published var userSubtitle = ""
    @Published var muted = false
    @Published var subtitles = true
    @Published var reportsPaused = false {
        didSet {
            if reportsPaused, reportBatch != nil { interrupt() }
            if !reportsPaused { reports.resume() }
        }
    }
    @Published var outputName = "听筒"
    @Published var speaker = false
    @Published var externalOutput = false
    @Published var degraded = false
    @Published var showSettings = false
    @Published var gateway = initialGateway()
    @Published var accessToken = ""
    @Published var provider = VoiceProvider(rawValue: UserDefaults.standard.string(forKey: "realtime-provider") ?? "") ?? .doubao
    @Published private(set) var target: Target?
    @Published private(set) var selectingTarget = false
    @Published private(set) var targetError: String?
    @Published private(set) var callID: String?
    @Published var toolsStatus = "Mac 工具未连接，暂时只能聊天"
    @Published var snapshotObservedAt: Date?
    var toolsStatusText: String {
        guard let snapshotObservedAt else { return toolsStatus }
        return toolsStatus + "\n项目概况更新于 " + snapshotObservedAt.formatted(date: .abbreviated, time: .standard)
    }
    @Published private(set) var wantsCall = false
    let avatar = AvatarScene()
    var realtime: OpenAIRealtime?
    let audio = AudioDuplex()
    var socket: URLSessionWebSocketTask?
    var voiceSession: URLSession?
    var receiveTask: Task<Void, Never>?
    var closingTask: Task<Void, Never>?
    var doubaoSender: DoubaoSender?
    var doubaoTools = false
    var doubaoReports = false
    var voiceGatewayEndpoint: URL?
    var gatewayRequestID: String?
    var gatewayTicket: RealtimeGatewayTicket?
    var outputEnded = false
    let mobileClient = MobileClient(vault: CredentialVault())
    var macBindingClient: RealtimeBindingClient?
    var macBindingTask: Task<Void, Never>?
    var bindingGeneration: UInt64 = 0
    var localConfiguration = RealtimeLocalConfiguration()
    var history = RealtimeHistory()
    var pendingHistory = Set<String>()
    var playedAfter: TimeInterval?
    var connectDeadline: Task<Void, Never>?
    var captureTask: Task<Void, Never>?
    private var recoveryTask: Task<Void, Never>?
    private var timer: Timer?
    private var observers: [NSObjectProtocol] = []
    var generation = 0
    var attempts = 0
    var connectedAt: TimeInterval?
    private var recovering = false
    var activeResponse = ""
    var ignored = Set<String>()
    var userSpeaking = false
    var phase: AvatarScene.Phase = .idle
    var lastActivity = ProcessInfo.processInfo.systemUptime
    var speechEndedAt: TimeInterval?
    var waitingFirstAudio = false
    var reduceMotion = false
    private var foreground = true
    var isCalling: Bool { wantsCall }
    let discussions: RealtimeDiscussionBook
    var discussionSession = UUID().uuidString
    var discussionCaptures: [String: DiscussionCapture] = [:]
    var currentDiscussion: DiscussionCapture?
    let operationStore: RealtimeOperationStore
    var operationTask: Task<Void, Never>?
    @Published var operationRecords: [RealtimeOperationObservation] = []
    @Published var recoveryStatus: String?
    let reports: RealtimeReportBook
    var reportTask: Task<Void, Never>?
    var reportSendTask: Task<Void, Never>?
    var reportDeadline: Task<Void, Never>?
    var reportBatch: RealtimeReportQueue.Batch?
    var reportResponse = ""
    var reportRetryAfter: TimeInterval = 0
    @Published var reportStatus: String?

    init(discussionURL: URL = URL.applicationSupportDirectory.appendingPathComponent("Norma/realtime-discussions.json"),
         operationURL: URL = URL.applicationSupportDirectory.appendingPathComponent("Norma/realtime-operations.json"),
         reportURL: URL = URL.applicationSupportDirectory.appendingPathComponent("Norma/realtime-reports.json")) {
        discussions = RealtimeDiscussionBook(store: RealtimeDiscussionStore(url: discussionURL))
        operationStore = RealtimeOperationStore(url: operationURL)
        reports = RealtimeReportBook(store: RealtimeReportStore(url: reportURL))
        accessToken = ServiceCredential.read(for: gateway)
        observers.append(NotificationCenter.default.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] note in
            let began = (note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt) == AVAudioSession.InterruptionType.began.rawValue
            if began { Task { @MainActor in self?.suspend("系统音频中断，点接通继续") } }
        })
        observers.append(NotificationCenter.default.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { [weak self] note in
            let removed = (note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt) == AVAudioSession.RouteChangeReason.oldDeviceUnavailable.rawValue
            Task { @MainActor in
                self?.updateRoute()
                if removed { self?.suspend("耳机已断开，确认音频输出后再接通") }
            }
        })
        observers.append(NotificationCenter.default.addObserver(forName: ProcessInfo.thermalStateDidChangeNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in self?.degraded = [.serious, .critical].contains(ProcessInfo.processInfo.thermalState) }
        })
    }

    func connect(machine: PairedMachine?, project: ProjectSnapshot?) {
        guard !wantsCall else { return }
        guard endpoint() != nil else { showSettings = true; return }
        if usesAutomaticAuthorization {
            showSettings = false
            if (machine == nil || project == nil), VoiceAuthorization.cached(accessToken,machineID:machine?.id) == nil {
                state = .paused; status = "请先配对并选择 Mac 项目，语音服务会自动配置。"; return
            }
        } else if accessToken.count < 24 { showSettings = true; return }
        target = machine.flatMap { machine in project.map { Target(machine: machine, project: $0) } }
        snapshotObservedAt = nil
        discussionSession = UUID().uuidString; discussionCaptures = [:]; currentDiscussion = nil
        wantsCall = true; attempts = 0; recovering = false
        history = RealtimeHistory(); subtitle = ""; userSubtitle = ""; ignored = []
        state = .connecting; status = usesAutomaticAuthorization ? "正在自动连接语音服务" : "正在准备麦克风"
        startTimer()
        recoveryTask = Task { await establish() }
    }

    private func endpoint() -> URL? { VoiceSessionTicket.endpoint(gateway) }

    func switchTarget(machine: PairedMachine, workspaceID: String) async {
        guard wantsCall, !selectingTarget else { return }
        if target?.machine.id == machine.id, target?.project.id == workspaceID { return }
        let current = generation
        selectingTarget = true; targetError = nil
        defer { if current == generation { selectingTarget = false } }
        do {
            // Browsing a cached list is not permission to dispatch: fetch and validate the current grant.
            let snapshot = try await mobileClient.snapshot(machine)
            let project = try snapshot.controlProject(workspaceID)
            guard current == generation, wantsCall, !Task.isCancelled else { return }
            if let update = localConfiguration.offlineUpdate { try realtime?.send(update) }
            interrupt()
            let previous = macBindingTask
            previous?.cancel()
            await previous?.value
            guard current == generation, wantsCall, !Task.isCancelled else { return }
            macBindingClient = nil
            target = Target(machine: machine, project: project)
            snapshotObservedAt = nil
            startOperationRecovery()
            if provider == .openai || doubaoTools { startMacBinding() }
            else { toolsStatus = "已选择 \(machine.name) · \(project.displayName)，豆包的 Mac 任务工具尚未接入" }
        } catch {
            guard current == generation, wantsCall, !Task.isCancelled else { return }
            targetError = "项目未切换，请检查电脑连接和控制权限；原通话目标保持不变。"
        }
    }

    private func establish() async {
        await closingTask?.value
        guard wantsCall, !Task.isCancelled else { return }
        generation += 1; let current = generation
        do {
            let token = try await preparedAccessToken()
            guard current == generation, wantsCall, !Task.isCancelled else { return }
            accessToken = token
            let permission = await AVAudioApplication.requestRecordPermission()
            guard current == generation, wantsCall, !Task.isCancelled else { return }
            guard permission else { throw CallError.message("请在系统设置中开启 Norma 的麦克风权限") }
            guard let endpoint = endpoint() else { throw CallError.message("请输入有效的 HTTPS 语音网关地址") }
            // Existing gateway contract; the phone never receives a long-lived OpenAI project key.
            var request = URLRequest(url: endpoint, timeoutInterval: 12)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
            request.httpBody = try JSONSerialization.data(withJSONObject: ["provider": provider.rawValue, "resume": attempts > 0])
            let session = URLSession(configuration: .ephemeral, delegate: VoiceGatewayRedirects(), delegateQueue: nil)
            defer { session.finishTasksAndInvalidate() }
            let (data, response) = try await session.data(for: request)
            guard current == generation, wantsCall, !Task.isCancelled else { return }
            guard (response as? HTTPURLResponse)?.statusCode == 200 else {
                if usesAutomaticAuthorization {
                    throw CallError.message((response as? HTTPURLResponse)?.statusCode == 401
                        ? "语音授权未通过，请连接已配对的 Mac 后重试。" : "语音服务暂时不可用，请稍后重试。")
                }
                if (response as? HTTPURLResponse)?.statusCode == 401 { showSettings = true }
                throw CallError.message("语音网关未能创建通话，请检查访问口令和服务状态")
            }
            let ticket = try VoiceSessionTicket(data: data, endpoint: endpoint, provider: provider)
            voiceGatewayEndpoint = endpoint
            try ServiceCredential.save(accessToken, for: gateway)
            UserDefaults.standard.set(gateway, forKey: "realtime-gateway")
            UserDefaults.standard.set(provider.rawValue, forKey: "realtime-provider")
            startOperationRecovery()
            if provider == .doubao { startDoubao(ticket: ticket, current: current); return }
            let rtc = OpenAIRealtime(); realtime = rtc
            rtc.onFailure = { [weak self] in if let self, generation == current { lostConnection() } }
            rtc.onEvent = { [weak self] event in
                guard let self, generation == current else { return }
                do { try handleOpenAI(event, current: current) } catch { lostConnection() }
            }
            connectDeadline = Task {
                // 20s = signaling deadline 15s + audio readiness allowance 5s.
                try? await Task.sleep(for: .seconds(20))
                if !Task.isCancelled, current == generation, state != .connected { lostConnection() }
            }
            status = "正在连接 Norma"
            try await rtc.start(token: ticket.token, speaker: speaker)
            guard current == generation, wantsCall else { return }
            callID = rtc.callID
            startMacBinding()
        } catch {
            guard current == generation, wantsCall, !Task.isCancelled else { return }
            if error is URLError { lostConnection(); return }
            hangup(); state = .failed; status = error.localizedDescription
        }
    }

    func configureDoubaoMedia(_ event: [String: Any]) {
        guard let id = event["call_id"] as? String, id.hasPrefix("doubao_"),
              (try? RealtimeBinding(callID: id, workspaceID: "media", generation: 1)) != nil,
              let capabilities = event["capabilities"] as? [String: Bool] else { return }
        callID = id
        doubaoTools = capabilities["task_tools"] == true
        doubaoReports = capabilities["reports"] == true
    }

    func toggleMute() {
        guard state == .connected else { return }
        muted.toggle(); realtime?.setMuted(muted)
        if provider == .doubao { audio.setMuted(muted); doubaoSender?.setMuted(muted) }
        if muted { userSpeaking = false; phase = .idle }
        status = muted ? "麦克风已静音" : "我在听"
    }

    func interrupt() {
        guard state == .connected else { return }
        if provider == .doubao { interruptDoubao(); return }
        let response = activeResponse
        _ = stopPlayback()
        do {
            if !response.isEmpty { try realtime?.send(["type": "response.cancel", "response_id": response]) }
            try realtime?.send(["type": "output_audio_buffer.clear"])
            phase = .listening; status = "我在听"
        } catch { lostConnection() }
    }

    @discardableResult func stopPlayback() -> Bool {
        interruptReport()
        realtime?.audio.silenceOutput(true); history.interrupt(activeResponse)
        if !activeResponse.isEmpty { ignored.insert(activeResponse) }
        if ignored.count > 64 { ignored = [activeResponse] }
        activeResponse = ""; playedAfter = nil; waitingFirstAudio = false; outputEnded = false
        return true
    }

    func toggleSpeaker() {
        guard !externalOutput else { return }
        if provider == .doubao {
            let current = generation, next = !speaker
            Task {
                do { try await audio.setSpeaker(next); if current == generation { updateRoute() } }
                catch { if current == generation { status = "音频输出切换失败" } }
            }
            return
        }
        do { try realtime?.audio.setSpeaker(!speaker); updateRoute() }
        catch { status = "音频输出切换失败" }
    }
    func updateRoute() {
        guard let output = AVAudioSession.sharedInstance().currentRoute.outputs.first else { return }
        speaker = output.portType == .builtInSpeaker
        externalOutput = ![.builtInReceiver, .builtInSpeaker].contains(output.portType)
        outputName = externalOutput ? output.portName : (speaker ? "扬声器" : "听筒")
    }
    func setForeground(_ active: Bool) { foreground = active }
    private func startTimer() {
        timer?.invalidate()
        // Playback observations stay alive in background; only the avatar stops updating.
        timer = Timer.scheduledTimer(withTimeInterval: 1 / 30, repeats: true) { [weak self] _ in
            Task { @MainActor in await self?.tick() }
        }
    }
    func tick() async {
        if wantsCall, !discussions.saving, discussions.failure != nil {
            suspend("讨论草稿未能保存，通话已暂停。原话仍在草稿中，请处理保存问题后再接通。")
            return
        }
        let current = generation, response = activeResponse
        let pcm = provider == .doubao ? await audio.snapshot() : nil
        guard current == generation, response == activeResponse else { return }
        let level = realtime?.audio.snapshot().level ?? pcm?.level ?? 0
        if outputEnded, pcm?.queuedMS == 0, !response.isEmpty, state == .connected {
            doubaoSender?.send(["type": "gateway.playback.done", "response_id": response])
            if (pcm?.playedMS ?? 0) > 0 { finishReportPlayback(response: response) }
            else if let batch = reportBatch {
                reports.unconfirmed(batch); reportStatus = "汇报未播放音频，结果仍在记录中。"
            }
            _ = stopPlayback(); phase = .listening; status = muted ? "麦克风已静音" : "我在听"
        }
        if let deadline = playedAfter, ProcessInfo.processInfo.systemUptime >= deadline, level < 0.015 {
            finishReportPlayback(response: activeResponse)
            history.markPlayed(activeResponse); activeResponse = ""; playedAfter = nil
            phase = .listening; status = muted ? "麦克风已静音" : "我在听"
        }
        if foreground { avatar.update(phase: phase, level: level, reduceMotion: reduceMotion, degraded: degraded, userSpeaking: userSpeaking) }
        beginReportIfReady()
    }
    func lostConnection() {
        guard wantsCall, !recovering else { return }
        // 30s = twice Doubao's 15s transport/audio readiness budget. Brief handshakes
        // do not replenish retries; a later outage after a stable call gets a fresh budget.
        if let connectedAt, ProcessInfo.processInfo.systemUptime - connectedAt >= 30 { attempts = 0 }
        recovering = true; cleanTransport(); attempts += 1
        guard attempts <= 3 else { hangup(); state = .failed; status = "网络未恢复，点接通重试"; return }
        state = .reconnecting; status = "网络中断，正在尝试恢复（\(attempts)/3）"
        recoveryTask?.cancel()
        recoveryTask = Task {
            // 1s, 2s, 4s exponential backoff; three consecutive failures bound automatic retries.
            try? await Task.sleep(for: .seconds(pow(2, Double(attempts - 1))))
            guard !Task.isCancelled, wantsCall else { return }
            recovering = false; await establish()
        }
    }
    func hangup() {
        wantsCall = false; recovering = false; recoveryTask?.cancel(); cleanTransport()
        macBindingClient = nil
        timer?.invalidate(); timer = nil
        state = .idle; phase = .idle; status = "通话已结束，已安排的任务继续执行"; muted = false
        avatar.update(phase: .idle, level: 0, reduceMotion: reduceMotion, degraded: degraded)
    }
    func suspend(_ message: String) {
        guard wantsCall else { return }
        hangup(); state = .paused; status = message
    }
    private func cleanTransport() {
        reportTask?.cancel(); reportTask = nil
        operationTask?.cancel(); operationTask = nil
        selectingTarget = false; targetError = nil
        generation += 1; closeDoubao(); connectDeadline?.cancel(); captureTask?.cancel(); captureTask = nil
        macBindingTask?.cancel(); macBindingTask = nil
        _ = stopPlayback(); realtime?.close(); realtime = nil; pendingHistory.removeAll()
        localConfiguration = RealtimeLocalConfiguration()
        doubaoTools = false; doubaoReports = false; voiceGatewayEndpoint = nil
        gatewayRequestID = nil; gatewayTicket = nil; reportRetryAfter = 0
        connectedAt = nil
        callID = nil; toolsStatus = "Mac 工具未连接，暂时只能聊天"
        phase = .idle; userSpeaking = false
    }
    enum CallError: LocalizedError {
        case message(String)
        var errorDescription: String? { switch self { case .message(let message): message } }
    }
}

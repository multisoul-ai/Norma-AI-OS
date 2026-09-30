@preconcurrency import AVFoundation
@preconcurrency import Speech
import SwiftUI
import NormaCore

/// System dictation only produces draft text; sending remains the composer's existing action.
@MainActor final class ChatDictation: ObservableObject {
    @Published private(set) var state = ChatVoiceDraft()
    @Published private(set) var partial = ""
    @Published private(set) var error: String?
    @Published private(set) var authorizing = false
    private var engine: AVAudioEngine?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var recognition: SFSpeechRecognitionTask?
    private var startup: Task<Void, Never>?
    private var deadline: Task<Void, Never>?
    private var finalText: String?
    private var draft = ""
    private var completion: ((String) -> Void)?
    var active: Bool { state.phase != .idle }
    var readyToRelease: Bool { finalText != nil }

    /// Ask on mode selection so permission sheets don't interrupt the user's first held utterance.
    func preparePermissions() async {
        guard !authorizing else { return }
        #if DEBUG
        if ProcessInfo.processInfo.environment["NORMA_CHAT_VOICE_FIXTURE"] != nil { return }
        #endif
        authorizing = true
        defer { authorizing = false }
        let speech = await withCheckedContinuation { continuation in
            // Speech may invoke this on a background queue; don't inherit MainActor isolation.
            SFSpeechRecognizer.requestAuthorization { @Sendable status in
                continuation.resume(returning: status == .authorized)
            }
        }
        guard !Task.isCancelled else { return }
        guard speech else { error = "请在系统设置中允许 Norma 使用语音识别"; return }
        let microphone = await AVAudioApplication.requestRecordPermission()
        guard !Task.isCancelled else { return }
        error = microphone ? nil : "请在系统设置中允许 Norma 使用麦克风"
    }

    func begin(draft: String, completion: @escaping (String) -> Void) {
        guard let token = state.begin() else { return }
        self.draft = draft; self.completion = completion; partial = ""; error = nil; finalText = nil
        startup = Task { [weak self] in
            guard let self else { return }
            #if DEBUG
            if let fixture = ProcessInfo.processInfo.environment["NORMA_CHAT_VOICE_FIXTURE"] {
                if fixture == "denied" { fail("请在系统设置中允许麦克风和语音识别"); return }
                guard state.didStart(token) else { return }
                finalText = fixture
                return
            }
            #endif
            guard state.token == token, !Task.isCancelled else { return }
            guard SFSpeechRecognizer.authorizationStatus() == .authorized,
                  AVAudioApplication.shared.recordPermission == .granted else {
                fail("请在系统设置中允许麦克风和语音识别"); return
            }
            do { try startCapture(token) }
            catch { fail("无法开始录音，请检查麦克风后重试") }
        }
    }

    private func startCapture(_ token: UUID) throws {
        let locale = Locale(identifier: Locale.preferredLanguages.first ?? "zh-CN")
        guard let recognizer = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer(), recognizer.isAvailable else {
            fail("当前语音识别不可用，请检查网络后重试"); return
        }
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.record, mode: .measurement, options: [.allowBluetoothHFP])
        try session.setActive(true)
        let engine = AVAudioEngine()
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.taskHint = .dictation
        self.request = request
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else {
            try? session.setActive(false, options: .notifyOthersOnDeactivation)
            fail("没有可用的麦克风，请检查音频设备"); return
        }
        // 1024 frames / 48kHz ≈ 21ms per microphone buffer; use the hardware's actual rate.
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { @Sendable buffer, _ in request.append(buffer) }
        self.engine = engine
        recognition = recognizer.recognitionTask(with: request) { @Sendable [weak self] result, error in
            let text = result?.bestTranscription.formattedString
            let final = result?.isFinal == true
            let failed = error != nil
            Task { @MainActor in self?.receive(text, final: final, failed: failed, token: token) }
        }
        engine.prepare(); try engine.start()
        guard state.didStart(token) else { cleanup(); return }
        deadline = Task { [weak self] in
            // System recognition supports short utterances; stop at 60s rather than recording indefinitely.
            try? await Task.sleep(for: .seconds(60))
            guard !Task.isCancelled, self?.state.token == token else { return }
            self?.release()
        }
    }

    func release() {
        let wasPreparing = state.phase == .preparing
        guard let token = state.release() else {
            if wasPreparing { cleanup(); error = "录音尚未开始，请授权后重新按住说话" }
            return
        }
        stopCapture()
        request?.endAudio()
        if let finalText { finish(finalText, token: token); return }
        deadline?.cancel()
        deadline = Task { [weak self] in
            // Bound final-result waiting to 8s; a stalled recognizer must not lock the composer.
            try? await Task.sleep(for: .seconds(8))
            guard !Task.isCancelled, self?.state.token == token else { return }
            self?.fail("语音识别超时，请重试或使用键盘输入")
        }
    }

    private func receive(_ text: String?, final: Bool, failed: Bool, token: UUID) {
        guard state.token == token else { return }
        if let text { partial = text }
        if final {
            finalText = text ?? ""
            stopCapture()
            if state.phase == .transcribing { finish(finalText ?? "", token: token) }
        } else if failed {
            fail("语音识别失败，请重试；原有草稿已保留")
        }
    }

    private func finish(_ text: String, token: UUID) {
        let result = state.complete(text, token: token, draft: draft)
        let completed = completion
        cleanup()
        if let result { completed?(result) }
        else { error = "没有听清，请按住按钮重新说话" }
    }

    func cancel() {
        state.cancel(); cleanup(); partial = ""
    }

    func interrupt() {
        guard active else { return }
        fail("录音已中断，请重新按住说话")
    }

    private func fail(_ message: String) {
        cancel(); error = message
    }

    private func stopCapture() {
        guard let engine else { return }
        if engine.isRunning { engine.stop() }
        engine.inputNode.removeTap(onBus: 0)
        self.engine = nil
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    private func cleanup() {
        startup?.cancel(); startup = nil
        deadline?.cancel(); deadline = nil
        stopCapture(); recognition?.cancel(); recognition = nil
        request = nil; finalText = nil; completion = nil
    }
}

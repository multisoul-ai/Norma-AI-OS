@preconcurrency import AVFoundation
@preconcurrency import Speech
import Foundation
import NormaCore

private enum IdeaTranscriptionError: LocalizedError {
    case denied, unavailable, empty, timedOut

    var errorDescription: String? {
        switch self {
        case .denied: "请在系统设置中允许 Norma 使用语音识别，录音仍保存在想法库。"
        case .unavailable: "当前语音识别不可用，请稍后重试；录音仍保存在想法库。"
        case .empty: "未识别到文字，可以播放原始录音后再次尝试。"
        case .timedOut: "转写等待超时，可以稍后重试；录音仍保存在想法库。"
        }
    }
}

private final class IdeaSpeechResult: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<String, Error>?
    private var task: SFSpeechRecognitionTask?

    init(_ continuation: CheckedContinuation<String, Error>) { self.continuation = continuation }

    func attach(_ task: SFSpeechRecognitionTask) {
        lock.lock()
        if continuation == nil {
            lock.unlock()
            task.cancel()
            return
        }
        self.task = task
        lock.unlock()
    }

    func finish(_ result: Result<String, Error>) {
        lock.lock()
        let continuation = self.continuation
        self.continuation = nil
        let task = self.task
        self.task = nil
        lock.unlock()
        guard let continuation else { return }
        task?.cancel()
        continuation.resume(with: result)
    }
}

private final class IdeaAudioSink: @unchecked Sendable {
    private let lock = NSLock()
    private let file: AVAudioFile
    private let recognition: SFSpeechAudioBufferRecognitionRequest?
    private let levelChanged: @Sendable (Float) -> Void
    private var lastLevelAt: TimeInterval = 0
    private(set) var writeFailed = false

    init(file: AVAudioFile, recognition: SFSpeechAudioBufferRecognitionRequest?,
         levelChanged: @escaping @Sendable (Float) -> Void) {
        self.file = file
        self.recognition = recognition
        self.levelChanged = levelChanged
    }

    func append(_ buffer: AVAudioPCMBuffer) {
        lock.lock()
        defer { lock.unlock() }
        do { try file.write(from: buffer) }
        catch { writeFailed = true }
        recognition?.append(buffer)
        let now = Date().timeIntervalSince1970
        // Ten readings per second are enough for a responsive waveform without a UI update per audio buffer.
        guard now - lastLevelAt >= 0.1, let channel = buffer.floatChannelData?[0] else { return }
        lastLevelAt = now
        let frames = Int(buffer.frameLength)
        guard frames > 0 else { return }
        var energy: Float = 0
        for index in stride(from: 0, to: frames, by: 16) { energy += channel[index] * channel[index] }
        let samples = Float((frames + 15) / 16)
        levelChanged(min(1, sqrt(energy / samples) * 6))
    }

    func failed() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return writeFailed
    }
}

/// Independent idea recording: Apple Speech supplies text, while a local CAF keeps the original audio.
@MainActor final class IdeaVoiceCapture: ObservableObject {
    enum Phase { case idle, preparing, recording, stopped }
    @Published private(set) var phase: Phase = .idle
    @Published private(set) var partial = ""
    @Published private(set) var finalTranscript: String?
    @Published private(set) var level: Float = 0
    @Published private(set) var recognitionNotice: String?
    @Published private(set) var error: String?
    private(set) var recordingURL: URL?
    private(set) var duration: TimeInterval = 0
    private(set) var startedAt: Date?
    private var engine: AVAudioEngine?
    private var sink: IdeaAudioSink?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var recognition: SFSpeechRecognitionTask?
    private var audioSessionActive = false
    private var token: UUID?
    private let directory: URL

    init(directory: URL) { self.directory = directory }

    static func transcribeFile(at url: URL) async throws -> String {
        let authorized = await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { @Sendable status in
                continuation.resume(returning: status == .authorized)
            }
        }
        guard authorized else { throw IdeaTranscriptionError.denied }
        let locale = Locale(identifier: Locale.preferredLanguages.first ?? "zh-CN")
        guard let recognizer = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer(),
              recognizer.isAvailable else { throw IdeaTranscriptionError.unavailable }
        let request = SFSpeechURLRecognitionRequest(url: url)
        request.shouldReportPartialResults = false
        request.requiresOnDeviceRecognition = recognizer.supportsOnDeviceRecognition
        request.taskHint = .dictation
        return try await withCheckedThrowingContinuation { continuation in
            let result = IdeaSpeechResult(continuation)
            let task = recognizer.recognitionTask(with: request) { @Sendable response, failure in
                if let failure { result.finish(.failure(failure)) }
                else if response?.isFinal == true {
                    let text = response?.bestTranscription.formattedString
                        .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                    result.finish(text.isEmpty ? .failure(IdeaTranscriptionError.empty) : .success(text))
                }
            }
            result.attach(task)
            Task {
                try? await Task.sleep(for: .seconds(60))
                result.finish(.failure(IdeaTranscriptionError.timedOut))
            }
        }
    }

    var elapsed: TimeInterval {
        phase == .recording ? Date().timeIntervalSince(startedAt ?? .now) : duration
    }

    func recover(_ url: URL) throws {
        guard phase == .idle, url.isFileURL, url.pathExtension == "caf",
              url.standardizedFileURL.deletingLastPathComponent() == directory.standardizedFileURL else {
            throw IdeaStoreError.invalidRecording
        }
        let file = try AVAudioFile(forReading: url)
        let seconds = Double(file.length) / file.processingFormat.sampleRate
        guard seconds.isFinite, seconds > 0 else { throw IdeaStoreError.invalidRecording }
        recordingURL = url
        duration = seconds
        phase = .stopped
        recognitionNotice = "上次录音已恢复，可保存为待转写。"
    }

    func start() async {
        guard phase == .idle else { return }
        let token = UUID()
        self.token = token
        phase = .preparing
        error = nil; recognitionNotice = nil; partial = ""; finalTranscript = nil
        let microphone = await AVAudioApplication.requestRecordPermission()
        guard self.token == token else { return }
        guard microphone else {
            phase = .idle
            error = "请在系统设置中允许 Norma 使用麦克风；未录音前不会保存空想法。"
            return
        }
        let speech = await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { @Sendable status in
                continuation.resume(returning: status == .authorized)
            }
        }
        guard self.token == token else { return }
        if !speech { recognitionNotice = "语音识别未授权，录音仍可保存为待转写。" }
        do { try startEngine(speechAuthorized: speech, token: token) }
        catch {
            phase = .idle
            self.error = "无法开始录音，请检查麦克风后重试。"
            cleanupAudio(removeFile: true)
        }
    }

    private func startEngine(speechAuthorized: Bool, token: UUID) throws {
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.record, mode: .measurement, options: [.allowBluetoothHFP])
        try session.setActive(true)
        audioSessionActive = true
        let engine = AVAudioEngine()
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { throw IdeaStoreError.invalidRecording }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("\(UUID().uuidString).caf")
        let file = try AVAudioFile(forWriting: url, settings: format.settings,
                                   commonFormat: format.commonFormat, interleaved: format.isInterleaved)
        var speechRequest: SFSpeechAudioBufferRecognitionRequest?
        if speechAuthorized {
            let locale = Locale(identifier: Locale.preferredLanguages.first ?? "zh-CN")
            if let recognizer = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer(), recognizer.isAvailable {
                let request = SFSpeechAudioBufferRecognitionRequest()
                request.shouldReportPartialResults = true
                request.requiresOnDeviceRecognition = recognizer.supportsOnDeviceRecognition
                request.taskHint = .dictation
                speechRequest = request
                self.request = request
                recognition = recognizer.recognitionTask(with: request) { @Sendable [weak self] result, failure in
                    let transcript = result?.bestTranscription.formattedString
                    let isFinal = result?.isFinal == true
                    let hasFailure = failure != nil
                    Task { @MainActor in
                        self?.receive(transcript: transcript, isFinal: isFinal,
                                      failed: hasFailure, token: token)
                    }
                }
            } else {
                recognitionNotice = "当前语音识别不可用，录音仍可保存为待转写。"
            }
        }
        let sink = IdeaAudioSink(file: file, recognition: speechRequest) { [weak self] amplitude in
            Task { @MainActor in self?.level = amplitude }
        }
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { @Sendable buffer, _ in sink.append(buffer) }
        self.engine = engine
        self.sink = sink
        recordingURL = url
        engine.prepare()
        try engine.start()
        startedAt = .now
        phase = .recording
    }

    private func receive(transcript: String?, isFinal: Bool, failed: Bool, token: UUID) {
        guard self.token == token else { return }
        if let transcript {
            partial = transcript
            if isFinal { finalTranscript = transcript }
        }
        if failed, finalTranscript == nil {
            recognitionNotice = "识别中断，录音可保存为待转写。"
        }
    }

    func stop() {
        guard phase == .recording else { return }
        duration = max(0.01, Date().timeIntervalSince(startedAt ?? .now))
        engine?.stop()
        engine?.inputNode.removeTap(onBus: 0)
        engine = nil
        request?.endAudio()
        if sink?.failed() == true { error = "录音写入失败，请重新录制。" }
        sink = nil
        deactivateAudioSession()
        phase = .stopped
        level = 0
    }

    func discard() {
        if phase == .recording { stop() }
        recognition?.cancel(); recognition = nil; request = nil
        cleanupAudio(removeFile: true)
        token = nil; phase = .idle
    }

    func finishSaving() {
        recognition?.cancel(); recognition = nil; request = nil
        cleanupAudio(removeFile: true) // IdeaStore has already copied the recording into its permanent directory.
        token = nil; phase = .idle
    }

    private func cleanupAudio(removeFile: Bool) {
        if engine?.isRunning == true { engine?.stop() }
        if engine != nil { engine?.inputNode.removeTap(onBus: 0) }
        engine = nil; sink = nil
        deactivateAudioSession()
        if removeFile, let recordingURL { try? FileManager.default.removeItem(at: recordingURL) }
        recordingURL = nil
    }

    private func deactivateAudioSession() {
        guard audioSessionActive else { return }
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        audioSessionActive = false
    }
}

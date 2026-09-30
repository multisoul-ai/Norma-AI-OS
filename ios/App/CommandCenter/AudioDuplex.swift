@preconcurrency import AVFoundation
import os

/// One AVAudioEngine owns input and output so iOS voice processing can cancel speaker echo.
final class AudioDuplex: @unchecked Sendable {
    struct Playback: Sendable { var level: Float = 0; var playedMS: Double = 0; var queuedMS: Double = 0 }
    private let queue = DispatchQueue(label: "norma.doubao.audio")
    private var engine: AVAudioEngine?
    private var player: AVAudioPlayerNode?
    private var renderedFrames: Int64 = 0
    private var scheduledFrames: Int64 = 0
    private var playbackGeneration = 0
    private var level: Float = 0
    private var captureMuted = false
    private var accumulated = Data()
    private var owner = -1
    private var configurationObserver: NSObjectProtocol?
    private var pendingStart: CheckedContinuation<Void, Error>?
    private let callbackLock = NSLock()
    private var frameCallback: (@Sendable (Data) -> Void)?
    private var errorCallback: (@Sendable (String) -> Void)?
    var onFrame: (@Sendable (Data) -> Void)? {
        get { callbackLock.withLock { frameCallback } }
        set { callbackLock.withLock { frameCallback = newValue } }
    }
    var onError: (@Sendable (String) -> Void)? {
        get { callbackLock.withLock { errorCallback } }
        set { callbackLock.withLock { errorCallback = newValue } }
    }

    func start(speaker: Bool, owner: Int) async throws {
        try await withCheckedThrowingContinuation { (result: CheckedContinuation<Void, Error>) in
            queue.async { [self] in
                do {
                    stopLocked()
                    self.owner = owner
                    let session = AVAudioSession.sharedInstance()
                    try session.setCategory(.playAndRecord, mode: .voiceChat, options: [.allowBluetoothHFP])
                    try session.setPreferredIOBufferDuration(0.01)
                    try session.setActive(true)
                    let external = session.currentRoute.outputs.contains { ![.builtInReceiver, .builtInSpeaker].contains($0.portType) }
                    if !external { try session.overrideOutputAudioPort(speaker ? .speaker : .none) }
                    let engine = AVAudioEngine(), player = AVAudioPlayerNode()
                    // Must be set before connecting the graph or installing capture taps.
                    try engine.inputNode.setVoiceProcessingEnabled(true)
                    engine.attach(player)
                    let output = AVAudioFormat(standardFormatWithSampleRate: 24000, channels: 1)!
                    engine.connect(player, to: engine.mainMixerNode, format: output)
                    let input = engine.inputNode.outputFormat(forBus: 0)
                    guard input.sampleRate > 0,
                          let pcm = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 16000, channels: 1, interleaved: true),
                          let converter = AVAudioConverter(from: input, to: pcm) else { throw AudioError.unavailable }
                    engine.inputNode.installTap(onBus: 0, bufferSize: 960, format: input) { [weak self] buffer, _ in
                        let capacity = AVAudioFrameCount(ceil(Double(buffer.frameLength) * 16000 / input.sampleRate)) + 16
                        guard let converted = AVAudioPCMBuffer(pcmFormat: pcm, frameCapacity: capacity) else { return }
                        let supplied = OSAllocatedUnfairLock(initialState: false)
                        var error: NSError?
                        converter.convert(to: converted, error: &error) { _, status in
                            if supplied.withLock({ sent in defer { sent = true }; return sent }) {
                                status.pointee = .noDataNow; return nil
                            }
                            status.pointee = .haveData; return buffer
                        }
                        if error != nil {
                            self?.queue.async { [weak self] in
                                guard let self, self.owner == owner else { return }
                                self.onError?("麦克风格式转换失败")
                            }
                            return
                        }
                        guard let samples = converted.int16ChannelData else { return }
                        let data = Data(bytes: samples[0], count: Int(converted.frameLength) * 2)
                        self?.queue.async { [weak self] in
                            guard let self, self.engine != nil, self.owner == owner else { return }
                            if !data.isEmpty {
                                self.pendingStart?.resume()
                                self.pendingStart = nil
                            }
                            if self.captureMuted { self.accumulated.removeAll(keepingCapacity: true); return }
                            Self.packetize(data, pending: &self.accumulated, emit: self.onFrame)
                        }
                    }
                    engine.mainMixerNode.installTap(onBus: 0, bufferSize: 480, format: nil) { [weak self] buffer, _ in
                        guard let channel = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return }
                        var sum: Float = 0
                        for i in 0..<Int(buffer.frameLength) { sum += channel[i] * channel[i] }
                        let rms = sqrt(sum / Float(buffer.frameLength))
                        self?.queue.async { [weak self] in
                            guard let self, self.owner == owner else { return }
                            self.level = min(1, rms * 9)
                        }
                    }
                    self.engine = engine; self.player = player
                    configurationObserver = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil) { [weak self, weak engine] _ in
                        self?.queue.async { [weak self, weak engine] in
                            guard let self, let engine, self.engine === engine, self.owner == owner, !engine.isRunning else { return }
                            do {
                                try engine.start()
                                if self.scheduledFrames > self.renderedFrames { self.player?.play() }
                            } catch { self.onError?("音频设备已变化，请重新接通") }
                        }
                    }
                    pendingStart = result
                    engine.prepare(); try engine.start()
                    // Hardware readiness is the first capture buffer, not engine.start() returning.
                    queue.asyncAfter(deadline: .now() + 3) { [weak self] in
                        guard let self, self.owner == owner, let pending = self.pendingStart else { return }
                        self.pendingStart = nil; self.stopLocked()
                        pending.resume(throwing: AudioError.unavailable)
                    }
                } catch { pendingStart = nil; stopLocked(); result.resume(throwing: error) }
            }
        }
    }

    func setMuted(_ muted: Bool) { queue.async { self.captureMuted = muted; self.accumulated.removeAll(keepingCapacity: true) } }

    static func packetize(_ data: Data, pending: inout Data, emit: ((Data) -> Void)?) {
        pending.append(data)
        // 16000 samples/sec × 2 bytes × 20 ms = 640 bytes per packet.
        while pending.count >= 640 {
            emit?(Data(pending.prefix(640)))
            // removeFirst advances Data's slice and retains consumed PCM across later appends.
            pending.removeSubrange(pending.startIndex..<pending.index(pending.startIndex, offsetBy: 640))
        }
    }

    func setSpeaker(_ speaker: Bool) async throws {
        try await withCheckedThrowingContinuation { (result: CheckedContinuation<Void, Error>) in
            queue.async {
                do {
                    let session = AVAudioSession.sharedInstance()
                    let external = session.currentRoute.outputs.contains { ![.builtInReceiver, .builtInSpeaker].contains($0.portType) }
                    if !external { try session.overrideOutputAudioPort(speaker ? .speaker : .none) }
                    result.resume()
                } catch { result.resume(throwing: error) }
            }
        }
    }

    func enqueue(_ data: Data) {
        queue.async { [self] in
            guard let player, data.count > 0, data.count.isMultiple(of: 2), data.count <= 480_000 else { return }
            if snapshotLocked().queuedMS + Double(data.count) / 48 > 6000 { onError?("播放缓冲过长，请重新接通"); return }
            let format = AVAudioFormat(standardFormatWithSampleRate: 24000, channels: 1)!
            let generation = playbackGeneration
            // Completion callbacks account for actual played audio in chunks <=20 ms.
            // This remains correct when the network leaves gaps between scheduled buffers.
            for offset in stride(from: 0, to: data.count, by: 960) {
                let count = min(960, data.count-offset) / 2
                guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(count)) else { return }
                buffer.frameLength = AVAudioFrameCount(count)
                data.withUnsafeBytes { raw in
                    for i in 0..<count {
                        let sample = raw.loadUnaligned(fromByteOffset: offset+i*2, as: Int16.self)
                        buffer.floatChannelData![0][i] = Float(Int16(littleEndian: sample)) / 32768
                    }
                }
                scheduledFrames += Int64(count)
                player.scheduleBuffer(buffer, completionCallbackType: .dataPlayedBack) { [weak self] _ in
                    self?.queue.async { [weak self] in
                        guard let self, self.playbackGeneration == generation else { return }
                        self.renderedFrames += Int64(count)
                    }
                }
            }
            if !player.isPlaying { player.play() }
        }
    }

    @discardableResult func interrupt() async -> Double {
        await withCheckedContinuation { result in
            queue.async { [self] in
                let played = snapshotLocked().playedMS
                player?.stop(); playbackGeneration += 1
                renderedFrames = 0; scheduledFrames = 0; level = 0
                result.resume(returning: played)
            }
        }
    }

    func snapshot() async -> Playback {
        await withCheckedContinuation { result in
            queue.async { result.resume(returning: self.snapshotLocked()) }
        }
    }

    private func snapshotLocked() -> Playback {
        let audible = min(scheduledFrames, max(0, renderedFrames))
        return Playback(level: level, playedMS: Double(audible)/24, queuedMS: Double(scheduledFrames-audible)/24)
    }

    func stop(owner: Int? = nil) async {
        await withCheckedContinuation { result in
            queue.async {
                if owner == nil || self.owner == owner { self.stopLocked() }
                result.resume()
            }
        }
    }
    private func stopLocked() {
        pendingStart?.resume(throwing: CancellationError()); pendingStart = nil
        if let configurationObserver { NotificationCenter.default.removeObserver(configurationObserver) }
        configurationObserver = nil
        engine?.inputNode.removeTap(onBus: 0); engine?.mainMixerNode.removeTap(onBus: 0)
        player?.stop(); engine?.stop(); engine = nil; player = nil
        playbackGeneration += 1; scheduledFrames = 0; renderedFrames = 0; level = 0
        owner = -1; accumulated.removeAll(keepingCapacity: false); captureMuted = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
    private enum AudioError: Error { case unavailable }
}

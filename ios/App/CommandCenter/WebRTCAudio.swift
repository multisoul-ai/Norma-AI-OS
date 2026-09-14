@preconcurrency import AVFoundation
@preconcurrency import WebRTC

/// WebRTC pulls/receives PCM at the hardware rate. The same voice-processing engine
/// renders the audio and samples its level; no network statistics drive the mouth.
// PCM state is locked; engine/delegate mutations are confined to WebRTC's native ADM thread.
final class WebRTCAudio: NSObject, RTCAudioDevice, @unchecked Sendable {
    private var delegate: RTCAudioDeviceDelegate?
    private var engine: AVAudioEngine?
    private var observer: NSObjectProtocol?
    private let lock = NSLock()
    private var playback = false, recording = false, silenced = false
    private var level: Float = 0
    private var captured: UInt64 = 0, rendered: UInt64 = 0
    private var levelAt: TimeInterval = 0
    private var requestedSpeaker = true
    var speaker: Bool {
        get { lock.withLock { requestedSpeaker } }
        set { lock.withLock { requestedSpeaker = newValue } }
    }
    var onError: (() -> Void)?
    private(set) var deviceInputSampleRate: Double = 48000
    private(set) var deviceOutputSampleRate: Double = 48000
    var inputIOBufferDuration: TimeInterval { AVAudioSession.sharedInstance().ioBufferDuration }
    var outputIOBufferDuration: TimeInterval { inputIOBufferDuration }
    var inputNumberOfChannels: Int { 1 }
    var outputNumberOfChannels: Int { 1 }
    var inputLatency: TimeInterval { AVAudioSession.sharedInstance().inputLatency }
    var outputLatency: TimeInterval { AVAudioSession.sharedInstance().outputLatency }
    var isInitialized: Bool { delegate != nil }
    var isPlayoutInitialized: Bool { isInitialized }
    var isRecordingInitialized: Bool { isInitialized }
    var isPlaying: Bool { lock.withLock { playback } }
    var isRecording: Bool { lock.withLock { recording } }

    func initialize(with delegate: RTCAudioDeviceDelegate) -> Bool { self.delegate = delegate; return true }
    func initializePlayout() -> Bool { isInitialized }
    func initializeRecording() -> Bool { isInitialized }
    func startPlayout() -> Bool {
        lock.withLock { playback = true }
        return startEngine()
    }
    func startRecording() -> Bool {
        lock.withLock { recording = true }
        return startEngine()
    }
    func stopPlayout() -> Bool {
        lock.withLock { playback = false; level = 0 }
        if !isRecording { stopEngine() }
        return true
    }
    func stopRecording() -> Bool {
        lock.withLock { recording = false }
        if !isPlaying { stopEngine() }
        return true
    }
    func terminateDevice() -> Bool {
        stopEngine(); delegate = nil
        lock.withLock { playback = false; recording = false; level = 0 }
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        return true
    }
    func silenceOutput(_ value: Bool) { lock.withLock { silenced = value; if value { level = 0 } } }
    func snapshot() -> (level: Float, captured: UInt64, rendered: UInt64) {
        lock.withLock {
            // A stopped render callback must not leave the mouth open indefinitely.
            (ProcessInfo.processInfo.systemUptime-levelAt < 0.1 ? level : 0, captured, rendered)
        }
    }
    func setSpeaker(_ value: Bool) throws {
        speaker = value
        let session = AVAudioSession.sharedInstance()
        if session.currentRoute.outputs.allSatisfy({ [.builtInReceiver, .builtInSpeaker].contains($0.portType) }) {
            try session.overrideOutputAudioPort(value ? .speaker : .none)
        }
    }

    // All engine mutations run on the native ADM thread, including route changes.
    private func startEngine() -> Bool {
        if engine?.isRunning == true { return true }
        guard let delegate else { return false }
        do {
            if let engine {
                // Recreating voice processing during a configuration change would trigger
                // another change. Reuse the graph, or reconnect if its PCM rate changed.
                guard engine.inputNode.outputFormat(forBus: 0).sampleRate == deviceInputSampleRate else { onError?(); return false }
                delegate.notifyAudioInputParametersChange(); delegate.notifyAudioOutputParametersChange()
                delegate.notifyAudioInputInterrupted(); delegate.notifyAudioOutputInterrupted()
                try engine.start()
                return true
            }
            stopEngine()
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .voiceChat, options: [.allowBluetoothHFP])
            try session.setPreferredSampleRate(48000)
            try session.setPreferredIOBufferDuration(0.01)
            try session.setActive(true)
            try setSpeaker(speaker)
            let engine = AVAudioEngine()
            try engine.inputNode.setVoiceProcessingEnabled(true)
            let rate = engine.inputNode.outputFormat(forBus: 0).sampleRate
            guard rate > 0, let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1) else { return false }
            deviceInputSampleRate = rate; deviceOutputSampleRate = rate
            // Each independent audio callback owns a fixed PCM scratch buffer.
            // 4096 / 48000 = 85 ms, above normal 10–20 ms hardware slices.
            var inputPCM = [Int16](repeating: 0, count: 4096)
            var outputPCM = [Int16](repeating: 0, count: 4096)
            let source = AVAudioSourceNode(format: format) { [weak self] _, timestamp, frames, buffers in
                guard let self else { return noErr }
                let state = self.lock.withLock { (self.playback, self.silenced) }
                let output = UnsafeMutableAudioBufferListPointer(buffers)
                for buffer in output { if let data = buffer.mData { memset(data, 0, Int(buffer.mDataByteSize)) } }
                guard frames <= 4096, state.0 else { return noErr }
                return outputPCM.withUnsafeMutableBufferPointer { pcm in
                    var list = AudioBufferList(mNumberBuffers: 1, mBuffers: AudioBuffer(mNumberChannels: 1,
                        mDataByteSize: frames * 2, mData: pcm.baseAddress))
                    var flags = AudioUnitRenderActionFlags()
                    let result = delegate.getPlayoutData(&flags, timestamp, 0, frames, &list)
                    guard result == noErr else { return result }
                    var sum: Float = 0
                    for i in 0..<Int(frames) {
                        let value = state.1 ? 0 : Float(pcm[i]) / 32768
                        for buffer in output { buffer.mData?.assumingMemoryBound(to: Float.self)[i] = value }
                        sum += value * value
                    }
                    self.lock.withLock {
                        self.level = min(1, sqrt(sum / Float(max(1, frames))) * 9)
                        self.levelAt = ProcessInfo.processInfo.systemUptime
                        self.rendered += UInt64(frames)
                    }
                    return noErr
                }
            }
            let sink = AVAudioSinkNode { [weak self] timestamp, frames, buffers in
                guard let self, self.isRecording, frames <= 4096,
                      let data = buffers.pointee.mBuffers.mData?.assumingMemoryBound(to: Float.self) else { return noErr }
                return inputPCM.withUnsafeMutableBufferPointer { pcm in
                    for i in 0..<Int(frames) { pcm[i] = Int16(max(-32768, min(32767, data[i] * 32768))) }
                    var list = AudioBufferList(mNumberBuffers: 1, mBuffers: AudioBuffer(mNumberChannels: 1,
                        mDataByteSize: frames * 2, mData: pcm.baseAddress))
                    var flags = AudioUnitRenderActionFlags()
                    let result = delegate.deliverRecordedData(&flags, timestamp, 1, frames, &list, nil, nil)
                    self.lock.withLock { self.captured += UInt64(frames) }
                    return result
                }
            }
            engine.attach(source); engine.attach(sink)
            engine.connect(source, to: engine.mainMixerNode, format: format)
            engine.connect(engine.inputNode, to: sink, format: format)
            self.engine = engine
            delegate.notifyAudioInputParametersChange(); delegate.notifyAudioOutputParametersChange()
            delegate.notifyAudioInputInterrupted(); delegate.notifyAudioOutputInterrupted()
            observer = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil) { [weak self, weak engine] _ in
                delegate.dispatchAsync { [weak self, weak engine] in
                    guard let self, let engine, self.engine === engine else { return }
                    _ = self.startEngine()
                }
            }
            engine.prepare(); try engine.start()
            return true
        } catch { stopEngine(); onError?(); return false }
    }
    private func stopEngine() {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        observer = nil; engine?.stop(); engine = nil
    }
}

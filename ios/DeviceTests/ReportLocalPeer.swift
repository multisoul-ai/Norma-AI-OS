import XCTest
@preconcurrency import WebRTC
@testable import Norma

// Test endpoint migrated from companion/ios/Tests/RealtimeTests.swift; local UDP only.
/// A local endpoint supplies quiet PCM through the native ADM without opening a second sound device.
final class ReportToneDevice: NSObject, RTCAudioDevice {
    private let toneLock = NSLock()
    private var toneEnabled = true
    func setTone(_ enabled: Bool) { toneLock.lock(); toneEnabled = enabled; toneLock.unlock() }
    private var delegate: RTCAudioDeviceDelegate?
    private var timer: DispatchSourceTimer?
    private var position = 0
    var deviceInputSampleRate: Double { 48000 }
    var deviceOutputSampleRate: Double { 48000 }
    var inputIOBufferDuration: TimeInterval { 0.01 }
    var outputIOBufferDuration: TimeInterval { 0.01 }
    var inputNumberOfChannels: Int { 1 }
    var outputNumberOfChannels: Int { 1 }
    var inputLatency: TimeInterval { 0 }
    var outputLatency: TimeInterval { 0 }
    var isInitialized: Bool { delegate != nil }
    var isPlayoutInitialized: Bool { isInitialized }
    var isRecordingInitialized: Bool { isInitialized }
    private(set) var isPlaying = false, isRecording = false
    func initialize(with delegate: RTCAudioDeviceDelegate) -> Bool { self.delegate = delegate; return true }
    func initializePlayout() -> Bool { true }
    func initializeRecording() -> Bool { true }
    func startPlayout() -> Bool { isPlaying = true; start(); return true }
    func startRecording() -> Bool { isRecording = true; start(); return true }
    func stopPlayout() -> Bool { isPlaying = false; return true }
    func stopRecording() -> Bool { isRecording = false; return true }
    func terminateDevice() -> Bool { timer?.cancel(); timer = nil; delegate = nil; return true }
    private func start() {
        guard timer == nil, let delegate else { return }
        delegate.notifyAudioInputParametersChange(); delegate.notifyAudioOutputParametersChange()
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "norma.report-test-tone"))
        timer.schedule(deadline: .now(), repeating: .milliseconds(10))
        timer.setEventHandler { [weak self] in delegate.dispatchAsync { [weak self] in self?.render() } }
        self.timer = timer; timer.resume()
    }
    private func render() {
        guard let delegate else { return }
        var samples = [Int16](repeating: 0, count: 480)
        samples.withUnsafeMutableBufferPointer { pcm in
            var flags = AudioUnitRenderActionFlags(), time = AudioTimeStamp()
            var list = AudioBufferList(mNumberBuffers: 1, mBuffers: AudioBuffer(mNumberChannels: 1, mDataByteSize: 960, mData: pcm.baseAddress))
            if isPlaying { _ = delegate.getPlayoutData(&flags, &time, 0, 480, &list) }
            toneLock.lock(); let enabled = toneEnabled; toneLock.unlock()
            for i in 0..<480 { pcm[i] = enabled ? Int16(sin(Double(position+i)/48000 * 2 * .pi * 220) * 800) : 0 }
            position += 480
            if isRecording { _ = delegate.deliverRecordedData(&flags, &time, 1, 480, &list, nil, nil) }
        }
    }
}

@MainActor final class ReportLocalPeer: NSObject, RTCPeerConnectionDelegate, RTCDataChannelDelegate {
    private let audio = ReportToneDevice()
    private var factory: RTCPeerConnectionFactory?
    private var peer: RTCPeerConnection?
    private var channel: RTCDataChannel?
    var onEvent: (([String: Any]) -> Void)?
    func setTone(_ enabled: Bool) { audio.setTone(enabled) }
    func send(_ event: [String: Any]) throws {
        guard let channel, channel.readyState == .open else { throw OpenAIRealtime.RTCError.closed }
        guard channel.sendData(RTCDataBuffer(data: try JSONSerialization.data(withJSONObject: event), isBinary: false)) else {
            throw OpenAIRealtime.RTCError.backpressure
        }
    }
    func answer(_ offer: RTCSessionDescription) async throws -> String {
        let factory = RTCPeerConnectionFactory(encoderFactory: nil, decoderFactory: nil, audioDevice: audio)
        self.factory = factory
        let config = RTCConfiguration(); config.sdpSemantics = .unifiedPlan
        let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        let peer = try XCTUnwrap(factory.peerConnection(with: config, constraints: constraints, delegate: self), "本地端点必须创建 peer")
        self.peer = peer
        peer.add(factory.audioTrack(withTrackId: "tone"), streamIds: ["test"])
        try await withCheckedThrowingContinuation { (result: CheckedContinuation<Void, Error>) in
            peer.setRemoteDescription(offer) { if let error = $0 { result.resume(throwing: error) } else { result.resume() } }
        }
        let answer: RTCSessionDescription = try await withCheckedThrowingContinuation { result in
            peer.answer(for: constraints) { sdp, error in
                if let sdp { result.resume(returning: sdp) } else { result.resume(throwing: error ?? OpenAIRealtime.RTCError.signaling) }
            }
        }
        try await withCheckedThrowingContinuation { (result: CheckedContinuation<Void, Error>) in
            peer.setLocalDescription(answer) { if let error = $0 { result.resume(throwing: error) } else { result.resume() } }
        }
        for _ in 0..<100 {
            if peer.iceGatheringState == .complete { break }
            try await Task.sleep(for: .milliseconds(50))
        }
        return try XCTUnwrap(peer.localDescription?.sdp, "本地端点必须给出 answer")
    }
    func close() { channel?.delegate = nil; channel?.close(); channel = nil; peer?.delegate = nil; peer?.close(); peer = nil; factory = nil }
    nonisolated func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {}
    nonisolated func dataChannel(_ dataChannel: RTCDataChannel, didReceiveMessageWith buffer: RTCDataBuffer) {
        if let event = try? JSONSerialization.jsonObject(with: buffer.data) as? [String: Any] {
            DispatchQueue.main.async { [weak self] in self?.onEvent?(event) }
        }
    }
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.peer != nil else { return }
            self.channel = dataChannel; dataChannel.delegate = self
            _ = dataChannel.sendData(RTCDataBuffer(data: Data("{\"type\":\"session.created\"}".utf8), isBinary: false))
        }
    }
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didAdd stream: RTCMediaStream) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didRemove stream: RTCMediaStream) {}
    nonisolated func peerConnectionShouldNegotiate(_ peerConnection: RTCPeerConnection) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceConnectionState) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceGatheringState) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {}
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}
}

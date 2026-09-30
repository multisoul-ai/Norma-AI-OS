import Foundation
@preconcurrency import WebRTC
import NormaCore

@MainActor final class OpenAIRealtime: NSObject, RTCPeerConnectionDelegate, RTCDataChannelDelegate {
    let audio = WebRTCAudio()
    private var factory: RTCPeerConnectionFactory?
    private(set) var peer: RTCPeerConnection?
    private var channel: RTCDataChannel?
    private var microphone: RTCAudioTrack?
    private var http: URLSession?
    private var closed = false
    private(set) var callID: String?
    var onEvent: (([String: Any]) -> Void)?
    var onFailure: (() -> Void)?

    func start(token: String, speaker: Bool) async throws {
        try prepare(speaker: speaker)
        let offer = try await makeOffer()
        guard !closed else { throw CancellationError() }
        // The destination is fixed; a compromised gateway cannot redirect this credential.
        var request = URLRequest(url: URL(string: "https://api.openai.com/v1/realtime/calls")!, timeoutInterval: 15)
        request.httpMethod = "POST"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/sdp", forHTTPHeaderField: "Content-Type")
        request.httpBody = Data(offer.utf8)
        let session = URLSession(configuration: .ephemeral)
        http = session
        let (data, response) = try await session.data(for: request)
        guard !closed else { throw CancellationError() }
        guard (response as? HTTPURLResponse)?.statusCode == 201,
              data.count < 128 * 1024, let sdp = String(data: data, encoding: .utf8), sdp.hasPrefix("v=0") else {
            throw RTCError.signaling
        }
        callID = (response as? HTTPURLResponse)?.value(forHTTPHeaderField: "Location").flatMap(RealtimeCallLocation.callID)
        try await acceptAnswer(sdp)
        session.finishTasksAndInvalidate(); http = nil
    }

    // These same SDP operations are exercised against a local peer in the test target.
    func prepare(speaker: Bool) throws {
        guard peer == nil, !closed else { throw RTCError.closed }
        audio.speaker = speaker
        audio.onError = { [weak self] in Task { @MainActor in if self?.closed == false { self?.onFailure?() } } }
        let factory = RTCPeerConnectionFactory(encoderFactory: nil, decoderFactory: nil, audioDevice: audio)
        self.factory = factory
        let configuration = RTCConfiguration()
        configuration.sdpSemantics = .unifiedPlan
        let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        guard let peer = factory.peerConnection(with: configuration, constraints: constraints, delegate: self) else { throw RTCError.signaling }
        self.peer = peer
        // The system voice-processing audio unit owns AEC. Avoid applying software AEC twice.
        let source = factory.audioSource(with: RTCMediaConstraints(mandatoryConstraints: nil,
            optionalConstraints: ["googEchoCancellation": "false", "googNoiseSuppression": "false", "googAutoGainControl": "false"]))
        let track = factory.audioTrack(with: source, trackId: "microphone")
        microphone = track; track.isEnabled = false
        peer.add(track, streamIds: ["voice"])
        guard let channel = peer.dataChannel(forLabel: "oai-events", configuration: RTCDataChannelConfiguration()) else { throw RTCError.signaling }
        self.channel = channel; channel.delegate = self
    }

    func makeOffer() async throws -> String {
        guard let peer, !closed else { throw RTCError.closed }
        let offer: RTCSessionDescription = try await withCheckedThrowingContinuation { result in
            peer.offer(for: RTCMediaConstraints(mandatoryConstraints: ["OfferToReceiveAudio": "true"], optionalConstraints: nil)) { sdp, error in
                if let sdp { result.resume(returning: sdp) } else { result.resume(throwing: error ?? RTCError.signaling) }
            }
        }
        try await withCheckedThrowingContinuation { (result: CheckedContinuation<Void, Error>) in
            peer.setLocalDescription(offer) { error in
                if let error { result.resume(throwing: error) } else { result.resume() }
            }
        }
        guard !closed else { throw CancellationError() }
        return offer.sdp
    }

    func acceptAnswer(_ sdp: String) async throws {
        guard let peer, !closed else { throw RTCError.closed }
        try await withCheckedThrowingContinuation { (result: CheckedContinuation<Void, Error>) in
            peer.setRemoteDescription(RTCSessionDescription(type: .answer, sdp: sdp)) { error in
                if let error { result.resume(throwing: error) } else { result.resume() }
            }
        }
    }

    func send(_ event: [String: Any]) throws {
        guard !closed, let channel, channel.readyState == .open else { throw RTCError.closed }
        let data = try JSONSerialization.data(withJSONObject: event)
        // Bound control traffic: no audio or video is serialized into this channel.
        guard data.count <= 16384, channel.bufferedAmount < 65536,
              channel.sendData(RTCDataBuffer(data: data, isBinary: false)) else { throw RTCError.backpressure }
    }
    func setMuted(_ value: Bool) { microphone?.isEnabled = !value }
    var microphoneEnabled: Bool { microphone?.isEnabled == true }
    func close() {
        guard !closed else { return }
        closed = true; audio.silenceOutput(true); microphone?.isEnabled = false
        http?.invalidateAndCancel(); http = nil
        channel?.delegate = nil; channel?.close(); channel = nil
        peer?.delegate = nil; peer?.close(); peer = nil; microphone = nil
        factory = nil; onEvent = nil; onFailure = nil
    }

    nonisolated func dataChannel(_ dataChannel: RTCDataChannel, didReceiveMessageWith buffer: RTCDataBuffer) {
        guard !buffer.isBinary, buffer.data.count <= 256 * 1024,
              let event = try? JSONSerialization.jsonObject(with: buffer.data) as? [String: Any] else {
            DispatchQueue.main.async { [weak self] in if self?.closed == false { self?.onFailure?() } }; return
        }
        // One FIFO dispatch preserves the SCTP event order (including restore acknowledgements).
        DispatchQueue.main.async { [weak self] in
            guard let self, !self.closed, self.channel === dataChannel else { return }
            self.onEvent?(event)
        }
    }
    nonisolated func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {
        let state = dataChannel.readyState
        DispatchQueue.main.async { [weak self] in
            guard let self, !self.closed else { return }
            if state == .closed { self.onFailure?() }
        }
    }
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCPeerConnectionState) {
        if [.failed, .disconnected, .closed].contains(newState) {
            DispatchQueue.main.async { [weak self] in if self?.closed == false { self?.onFailure?() } }
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
    nonisolated func peerConnection(_ peerConnection: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {}
    enum RTCError: Error { case signaling, closed, backpressure }
}

/// Keeps conversation order independently of asynchronous transcription order.
/// Only final user text and completely played assistant turns may seed a new session.
struct RealtimeHistory {
    private struct Item {
        var id: String, role: String, response: String = "", text: String = ""
        var complete = false, played = false, interrupted = false
    }
    private var items: [Item] = []
    mutating func observe(_ event: [String: Any]) {
        let type = event["type"] as? String ?? ""
        if ["conversation.item.added", "response.output_item.added"].contains(type),
           let item = event["item"] as? [String: Any], let id = item["id"] as? String,
           item["type"] as? String == "message", let role = item["role"] as? String, ["user", "assistant"].contains(role) {
            if !items.contains(where: { $0.id == id }) { items.append(Item(id: id, role: role)) }
            if let response = event["response_id"] as? String, let index = items.firstIndex(where: { $0.id == id }) { items[index].response = response }
            if items.count > 24 { items.removeFirst(items.count-24) }
        }
        if ["conversation.item.input_audio_transcription.completed", "response.output_audio_transcript.done"].contains(type),
           let id = event["item_id"] as? String, let text = event["transcript"] as? String,
           let index = items.firstIndex(where: { $0.id == id }) {
            items[index].text = String(text.prefix(2000)); items[index].complete = true
        }
    }
    mutating func markPlayed(_ response: String) {
        for index in items.indices where items[index].response == response { items[index].played = true }
    }
    mutating func interrupt(_ response: String) {
        for index in items.indices where items[index].response == response { items[index].interrupted = true }
    }
    var replayEvents: [[String: Any]] {
        var events: [[String: Any]] = [], bytes = 0
        for item in items.reversed() where item.complete && !item.text.isEmpty && (item.role == "user" || (item.played && !item.interrupted)) {
            let event: [String: Any] = ["type": "conversation.item.create", "item": ["id": item.id, "type": "message", "role": item.role,
                "content": [["type": item.role == "user" ? "input_text" : "output_text", "text": item.text]]]]
            guard let data = try? JSONSerialization.data(withJSONObject: event), bytes + data.count <= 32768 else { break }
            // Half the data-channel high-water mark leaves room for new control events.
            bytes += data.count; events.append(event)
        }
        return events.reversed()
    }
}

import Foundation

/// One ordered sender keeps microphone frames and mute/interrupt controls on the same socket.
final class DoubaoSender: @unchecked Sendable {
    private struct Message: Sendable {
        let data: Data
        var captureRevision: UInt64?
        var unmuteRevision: UInt64?
    }
    private let lock = NSLock()
    private var revision: UInt64 = 0
    private var muted = false
    private let continuation: AsyncStream<Message>.Continuation
    private var task: Task<Void, Never>?
    private let onFailure: @Sendable () -> Void

    init(socket: URLSessionWebSocketTask, muted initiallyMuted: Bool, onFailure: @escaping @Sendable () -> Void) {
        // Two measured hardware batches × 6 frames = 12; four slots reserve control messages.
        // Even when full, nothing is silently dropped: the call pauses and releases the transport.
        let (stream, continuation) = AsyncStream<Message>.makeStream(bufferingPolicy: .bufferingOldest(16))
        self.continuation = continuation; self.onFailure = onFailure; self.muted = initiallyMuted
        task = Task.detached(priority: .userInitiated) { [weak self] in
            for await message in stream {
                guard !Task.isCancelled, let self else { break }
                if let revision = message.captureRevision,
                   !lock.withLock({ !muted && self.revision == revision }) { continue }
                do {
                    try await socket.send(.string(String(decoding: message.data, as: UTF8.self)))
                    if let revision = message.unmuteRevision {
                        lock.withLock { if self.revision == revision { muted = false } }
                    }
                } catch { if !Task.isCancelled { onFailure() }; break }
            }
        }
    }

    func append(_ frame: Data) {
        guard let current = lock.withLock({ muted ? nil : revision }),
              let data = try? JSONSerialization.data(withJSONObject: ["type": "input_audio_buffer.append", "audio": frame.base64EncodedString()]) else { return }
        enqueue(Message(data: data, captureRevision: current))
    }

    func send(_ event: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: event) else { onFailure(); return }
        enqueue(Message(data: data))
    }

    func setMuted(_ muted: Bool) {
        let current = lock.withLock { revision &+= 1; self.muted = true; return revision }
        let type = muted ? "input_audio_mute.commit" : "input_audio_unmute.commit"
        let data = Data("{\"type\":\"\(type)\"}".utf8)
        enqueue(Message(data: data, unmuteRevision: muted ? nil : current))
    }

    func close() {
        let oldTask = lock.withLock { revision &+= 1; muted = true; defer { task = nil }; return task }
        continuation.finish(); oldTask?.cancel()
    }

    private func enqueue(_ message: Message) {
        if case .dropped = continuation.yield(message) { close(); onFailure() }
    }
}

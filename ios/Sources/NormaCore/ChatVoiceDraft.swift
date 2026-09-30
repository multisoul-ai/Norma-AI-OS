import Foundation

/// A hold belongs to one draft; cancelled or duplicate recognition callbacks cannot edit it.
public struct ChatVoiceDraft {
    public enum Phase: Sendable { case idle, preparing, recording, transcribing }
    public private(set) var phase: Phase = .idle
    public private(set) var token: UUID?

    public init() {}

    public mutating func begin() -> UUID? {
        guard phase == .idle else { return nil }
        token = UUID(); phase = .preparing
        return token
    }

    public mutating func didStart(_ token: UUID) -> Bool {
        guard self.token == token, phase == .preparing else { return false }
        phase = .recording
        return true
    }

    public mutating func release() -> UUID? {
        guard phase == .recording else {
            if phase == .preparing { cancel() }
            return nil
        }
        phase = .transcribing
        return token
    }

    public mutating func complete(_ text: String, token: UUID, draft: String) -> String? {
        guard self.token == token, phase == .transcribing else { return nil }
        cancel()
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return nil }
        return draft + (draft.isEmpty || draft.last?.isWhitespace == true ? "" : "\n") + text
    }

    public mutating func cancel() {
        token = nil; phase = .idle
    }
}

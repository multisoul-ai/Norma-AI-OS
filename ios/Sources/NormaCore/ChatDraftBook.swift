import Foundation
import Observation

/// One app-owned draft book: view dismissal never cancels already queued persistence.
@MainActor @Observable public final class ChatDraftBook {
    public private(set) var drafts: [ChatDraft] = []
    public private(set) var loaded = false
    public private(set) var saving = false
    public private(set) var failure: String?
    private let store: ChatDraftStore
    private var loading = false
    private var write: Task<Void, Never>?
    private var generation = UUID()

    public init(store: ChatDraftStore) { self.store = store }
    public func load() async {
        guard !loaded, !loading else { return }
        loading = true
        defer { loading = false }
        do { drafts = try await store.load(); loaded = true; failure = nil }
        catch { failure = "无法读取草稿，原始记录已保留。" }
    }
    public func draft(for scope: ChatDraft.Scope) -> ChatDraft? { drafts.first { $0.scope == scope } }
    public func update(_ text: String, for scope: ChatDraft.Scope, label: String? = nil) {
        // SwiftUI may echo the existing value when focus/enabled state changes.
        // Only a real text edit creates a new revision; otherwise a send receipt cannot clear it.
        guard loaded, (draft(for: scope)?.text ?? "") != text else { return }
        let label = label ?? draft(for: scope)?.label
        drafts.removeAll { $0.scope == scope }
        if !text.isEmpty { drafts.append(ChatDraft(id: UUID().uuidString, scope: scope, text: text, editedAt: .now, label: label)) }
        persist()
    }
    public func remove(id: String) {
        guard loaded, drafts.contains(where: { $0.id == id }) else { return }
        drafts.removeAll { $0.id == id }; persist()
    }
    public func retry() async {
        if loaded { persist() } else { await load() }
    }
    public func flush() async throws {
        guard loaded else { throw CocoaError(.fileReadUnknown) }
        while true {
            let observed = generation
            await write?.value
            if observed == generation { break }
        }
        if failure != nil { throw CocoaError(.fileWriteUnknown) }
    }
    public func confirm(_ command: PendingMobileCommand, receipt: MobileCommandReceipt) async throws {
        try receipt.validate(for: command)
        guard receipt.state == "succeeded", let id = command.draftID,
              let (card, text) = command.request.command.messageContent else { return }
        guard loaded else { throw CocoaError(.fileReadUnknown) }
        let scope = ChatDraft.Scope(machineID: command.machineID, workspaceID: command.request.workspaceID,
                                    routeID: command.request.routeID, nodeID: card.nodeID, runID: card.runID, sessionID: card.sessionID,
                                    feedbackFingerprint: command.request.command.feedback?.fingerprint)
        if let draft = draft(for: scope), draft.id == id, draft.text == text {
            remove(id: id)
        }
        try await flush() // A previous clear may still be unsaved even when its draft is already absent in memory.
    }
    private func persist() {
        // ponytail: rewrite at most 128 drafts / 4 MiB; use per-draft files if measured typing IO warrants it.
        generation = UUID(); saving = true; failure = nil
        guard write == nil else { return }
        write = Task {
            // Coalesce edits during IO into one latest snapshot instead of accumulating per-keystroke tasks.
            while true {
                let serial = generation
                let snapshot = drafts
                do {
                    try await store.replace(snapshot)
                    if generation == serial { failure = nil; break }
                } catch {
                    if generation == serial { failure = "草稿尚未保存。请保留当前页面，缩短内容或重试。"; break }
                }
            }
            saving = false; write = nil
        }
    }
}

import SwiftUI
import NormaCore

@MainActor @Observable final class WorkThreadReviewModel {
    var operation: ThreadTaskOutboxOperation
    var revisionTask: ThreadTaskOutboxOperation?
    var draft: WorkThreadDraft?
    var error: String?
    var saving = false
    private(set) var loaded = false
    private var dirtyDraft = false
    private var draftWrite: Task<Void, Never>?
    private var backgroundWrite: UIBackgroundTaskIdentifier = .invalid
    private var backgroundGeneration = UUID()

    init(operation: ThreadTaskOutboxOperation) { self.operation = operation }

    func load(_ store: CollaborationWorkspaceStore) async {
        do {
            let current = try await store.threadTask(operationID: operation.id)
            if !loaded { draft = current.reviewDraft }
            operation = current; loaded = true
            if let id = current.revisionOperationID { revisionTask = try await store.threadTask(operationID: id) }
        } catch { self.error = error.localizedDescription }
    }

    func updateDraft(_ text: String, store: CollaborationWorkspaceStore) {
        guard loaded, !saving, (draft?.text ?? "") != text else { return }
        draft = text.isEmpty ? nil : WorkThreadDraft(text: text)
        dirtyDraft = true
        guard draftWrite == nil else { return }
        draftWrite = Task {
            while dirtyDraft {
                dirtyDraft = false
                let captured = draft
                do {
                    try await store.saveThreadReviewDraft(captured, operationID: operation.id)
                    if !dirtyDraft { error = nil }
                } catch { self.error = "修改意见尚未保存，请保留当前页面并重试。" }
            }
            draftWrite = nil
        }
    }

    func accept(_ store: CollaborationWorkspaceStore) async {
        guard !saving else { return }
        saving = true; error = nil
        defer { saving = false }
        do { operation = try await store.requestThreadAcceptance(operationID: operation.id, retryRejected: true) }
        catch { self.error = error.localizedDescription }
    }

    func flushDraft() async {
        while let write = draftWrite { await write.value }
    }

    func finishDraftWrites() {
        guard draftWrite != nil, backgroundWrite == .invalid else { return }
        let generation = UUID(); backgroundGeneration = generation
        backgroundWrite = UIApplication.shared.beginBackgroundTask(withName: "Save review feedback") { [weak self] in
            Task { @MainActor in self?.endDraftWrites(generation) }
        }
        Task { await flushDraft(); endDraftWrites(generation) }
    }

    private func endDraftWrites(_ generation: UUID) {
        guard generation == backgroundGeneration, backgroundWrite != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundWrite); backgroundWrite = .invalid
    }

    func revise(_ store: CollaborationWorkspaceStore) async -> Bool {
        guard !saving, let draft else { return false }
        saving = true; error = nil
        defer { saving = false }
        do {
            await flushDraft()
            // Retry the exact captured draft before the atomic message/task write. The thread composer is independent.
            try await store.saveThreadReviewDraft(draft, operationID: operation.id)
            revisionTask = try await store.requestThreadRevision(operationID: operation.id, draft: draft)
            self.draft = nil
            await load(store)
            return true
        } catch { self.error = error.localizedDescription; return false }
    }

    var canDecide: Bool {
        loaded && !saving && operation.revisionOperationID == nil && (operation.acceptance == nil
            || (operation.acceptance?.receipt?.state == "rejected" && operation.acceptance?.acceptance == nil))
    }
    var status: String { Self.status(for: operation) }

    static func status(for operation: ThreadTaskOutboxOperation) -> String {
        if operation.revisionOperationID != nil { return "已退回修改" }
        if let acceptance = operation.acceptance { return acceptance.statusLabel }
        return switch operation.receipt?.execution?.execution.delivery?.outcome.state {
        case .failed: "执行失败"
        case .interrupted: "已停止"
        default: "待验收"
        }
    }
    var acceptanceRestriction: String? {
        switch operation.receipt?.execution?.execution.delivery?.outcome.state {
        case .completed: nil
        case .failed: "本次执行失败，无法验收通过。可查看已保存的文件并退回修改。"
        case .interrupted: "本次执行已停止，无法验收通过。可查看已保存的文件并退回修改。"
        case nil: "尚未收到完整成果，暂时无法验收。"
        }
    }
    var resultSymbol: String {
        switch operation.receipt?.execution?.execution.delivery?.outcome.state {
        case .completed: "checkmark.circle"
        case .failed: "exclamationmark.circle"
        case .interrupted: "stop.circle"
        case nil: "doc.text"
        }
    }
    var resultColor: Color {
        switch operation.receipt?.execution?.execution.delivery?.outcome.state {
        case .completed: IdeaPalette.positiveText
        case .failed: .red
        default: IdeaPalette.secondary
        }
    }
}

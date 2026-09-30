import Foundation
import SQLite3

public enum ThreadMentionError: LocalizedError {
    case unknown(String), ambiguous(String), normaUnavailable, contextTooLarge
    public var errorDescription: String? {
        switch self {
        case let .unknown(name): "工作空间中没有 \(name)，请通过 @ 选择参与的 Agent。消息仍在草稿中。"
        case let .ambiguous(name): "有多位 Agent 叫 \(name)，请通过 @ 选择具体项目中的成员。"
        case .normaUnavailable: "暂时无法在此线程联系 Norma，消息已保留在草稿中。"
        case .contextTooLarge: "消息与工作简报过长，暂时无法交给 Agent。请精简后重试，原文已保留。"
        }
    }
}

public enum ThreadMentions {
    static let normaPattern = "(?<![\\p{L}\\p{N}_@])@Norma(?=$|[\\s，,。.!?！？:：;；])"
    public static func requestsNorma(_ text: String) -> Bool {
        text.range(of: normaPattern, options: [.regularExpression, .caseInsensitive]) != nil
    }
    public static func contains(_ target: WorkspaceAgentReference, in text: String) -> Bool {
        let pattern = "(?<![\\p{L}\\p{N}_@])@" + NSRegularExpression.escapedPattern(for: target.name) + "(?=$|[\\s，,。.!?！？:：;；])"
        return text.range(of: pattern, options: [.regularExpression, .caseInsensitive]) != nil
    }

    public static func resolve(_ text: String, workspace: CollaborationWorkspace, selected: [WorkspaceAgentReference] = []) throws -> [WorkspaceAgentReference] {
        var remainder = text, targets: [WorkspaceAgentReference] = []
        for selected in selected where contains(selected, in: text) {
            guard let current = workspace.agents.first(where: { $0.id == selected.id }) else { throw ThreadDeliveryError.targetChanged }
            targets.append(current)
            let pattern = "(?<![\\p{L}\\p{N}_@])@" + NSRegularExpression.escapedPattern(for: selected.name) + "(?=$|[\\s，,。.!?！？:：;；])"
            remainder = remainder.replacingOccurrences(of: pattern, with: "", options: [.regularExpression, .caseInsensitive])
        }
        let regex = try NSRegularExpression(pattern: "(?<![\\p{L}\\p{N}_@])@([^\\s@，,。.!?！？:：;；]+)")
        for match in regex.matches(in: remainder, range: NSRange(remainder.startIndex..., in: remainder)) {
            let name = (remainder as NSString).substring(with: match.range(at: 1))
            if name.lowercased() == "norma" { throw ThreadMentionError.normaUnavailable }
            let matches = workspace.agents.filter { $0.name.caseInsensitiveCompare(name) == .orderedSame }
            guard let target = matches.first else { throw ThreadMentionError.unknown(name) }
            guard matches.count == 1 else { throw ThreadMentionError.ambiguous(name) }
            targets.append(target)
        }
        var seen = Set<String>()
        return targets.filter { seen.insert($0.id).inserted }
    }
}

extension CollaborationWorkspaceStore {
    /// The UI sends through this single transaction; discussion creates no tasks.
    public func postThreadMessage(threadID: String, id: String, text: String,
                                   selected: [WorkspaceAgentReference] = [], clearingDraftRevision: String? = nil) throws -> WorkThreadMessage {
        try threadDatabase(writing: true) { db in
            // A retry of an already committed message never reinterprets it or creates new tasks.
            if try !threadRows(db, "SELECT id,workspace_id,payload FROM work_messages WHERE id=?", [id]).isEmpty {
                return try appendMessage(db, threadID: threadID, id: id, text: text, at: .now, clearingDraftRevision: clearingDraftRevision)
            }
            let thread = try requireThread(db, threadID)
            guard let row = try threadRows(db, "SELECT id,id,configuration FROM collaboration_workspaces WHERE id=?", [thread.workspaceID]).first else { throw WorkThreadError.notFound }
            let workspace = try decode(CollaborationWorkspace.self, row)
            try validate(workspace)
            let recipients = try ThreadMentions.resolve(text, workspace: workspace, selected: selected)
            let message = try appendMessage(db, threadID: threadID, id: id, text: text, at: .now, clearingDraftRevision: clearingDraftRevision)
            if !recipients.isEmpty {
                let context = try taskContext(db, thread: thread, message: message)
                for target in recipients {
                    _ = try enqueueThreadTask(db, sourceMessageID: id, operationID: UUID().uuidString, target: target,
                                              task: message.text, contextVersion: context.version, context: context.text)
                }
            }
            return message
        }
    }

    func taskContext(_ db: OpaquePointer, thread: WorkThread, message: WorkThreadMessage, addressedAgentIDs: [String] = []) throws -> (version: UInt64, text: String) {
        let facts = try currentFacts(db, workspaceID: thread.workspaceID).filter { $0.kind != .goal || $0.threadID == thread.id }
        let history = try messages(db, thread: thread).filter { $0.id != message.id }
        // Snapshot revision: one new message + prior messages + revisions of included facts.
        let version = UInt64(1 + history.count + facts.reduce(0) { $0 + $1.version })
        var snapshot = ThreadTaskContext(threadID: thread.id, workspaceID: thread.workspaceID, title: thread.title,
            sourceIdea: thread.sourceExcerpt, facts: facts, recentMessages: [], omittedMessages: history.count)
        snapshot.addressedAgentIDs = addressedAgentIDs.isEmpty ? nil : addressedAgentIDs
        func encoded() throws -> String { String(decoding: try JSONEncoder().encode(snapshot), as: UTF8.self) }
        var result = try encoded()
        // The protocol's 32 KiB is shared by the task and context, not an independent context allowance.
        let available = 32 * 1024 - message.text.utf8.count
        guard result.utf8.count <= available else { throw ThreadMentionError.contextTooLarge }
        // Twelve messages cover at most six recent exchanges; older history stays on the phone.
        for prior in history.suffix(12).reversed() {
            snapshot.recentMessages.insert(prior, at: 0); snapshot.omittedMessages -= 1
            let candidate = try encoded()
            guard candidate.utf8.count <= available else { break }
            result = candidate
        }
        return (version, result)
    }
}

public struct ThreadTaskContext: Codable, Sendable {
    public let threadID: String
    public let workspaceID: String
    public let title: String
    public let sourceIdea: String?
    public let facts: [WorkspaceFact]
    public var recentMessages: [WorkThreadMessage]
    public var omittedMessages: Int
    public var addressedAgentIDs: [String]? = nil
}

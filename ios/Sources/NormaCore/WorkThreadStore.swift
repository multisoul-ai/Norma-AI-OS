import Foundation
import SQLite3

public struct WorkThread: Codable, Hashable, Identifiable, Sendable {
    public let id: String
    public let workspaceID: String
    public let title: String
    public let sourceIdeaID: String?
    public let sourceExcerpt: String?
    public let originDeviceID: String
    public let createdAt: Date
}

public enum WorkMessageState: String, Codable, Sendable { case pending, delivered }

public struct WorkThreadDraft: Codable, Sendable, Equatable {
    public let text: String
    public let revision: String
    public let mentions: [WorkspaceAgentReference]?
    public let followupTaskID: String?
    public init(text: String, mentions: [WorkspaceAgentReference] = [], followupTaskID: String? = nil) {
        self.text = text; self.mentions = mentions.isEmpty ? nil : mentions; revision = UUID().uuidString
        self.followupTaskID = followupTaskID
    }
    var valid: Bool {
        let targets = mentions ?? []
        return (!text.isEmpty || followupTaskID != nil) && text.count <= 100_000 && UUID(uuidString: revision) != nil
            && (followupTaskID.map { UUID(uuidString: $0) != nil } ?? true)
            && targets.count <= 256 && Set(targets.map(\.id)).count == targets.count
            && targets.allSatisfy { validID($0.machineID) && validID($0.projectID) && $0.definitionID > 0
                && !$0.name.isEmpty && $0.name.count <= 256 && $0.profile.count <= 256 }
    }
}

public struct WorkThreadMessage: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let workspaceID: String
    public let threadID: String
    public let author: String
    public let originDeviceID: String
    public let text: String
    public let createdAt: Date
    public let state: WorkMessageState
    public var taskOperationID: String? = nil
    public var replyToMessageID: String? = nil
}

public enum WorkspaceFactKind: String, Codable, CaseIterable, Sendable { case goal, decision, fact }

public struct WorkspaceFact: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let workspaceID: String
    public let threadID: String
    public let kind: WorkspaceFactKind
    public let text: String
    public let version: Int
    public let author: String
    public let sourceMessageID: String?
    public let createdAt: Date
}

public enum WorkThreadError: LocalizedError {
    case invalidContent, notFound, archived, conflict, damagedStore
    public var errorDescription: String? {
        switch self {
        case .invalidContent: "内容或来源无效，请检查后重试。"
        case .notFound: "找不到这条工作记录，原有内容已保留。"
        case .archived: "工作空间已归档，请先重开再继续。"
        case .conflict: "这条记录已经变化，请重新读取后再修改。"
        case .damagedStore: "无法读取工作记录，原有数据已保留。"
        }
    }
}

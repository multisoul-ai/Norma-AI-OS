import Foundation
import SQLite3

public struct WorkspaceProjectReference: Codable, Equatable, Sendable, Identifiable {
    public let machineID: String
    public let projectID: String
    public let name: String
    public let machineName: String
    public var id: String { machineID + ":" + projectID }
    public init(machineID: String, projectID: String, name: String, machineName: String) {
        self.machineID = machineID; self.projectID = projectID; self.name = name; self.machineName = machineName
    }
}

public struct WorkspaceAgentReference: Codable, Equatable, Sendable, Identifiable {
    public let machineID: String
    public let projectID: String
    public let definitionID: Int64
    public let name: String
    public let profile: String
    public var projectReferenceID: String { machineID + ":" + projectID }
    public var id: String { projectReferenceID + ":" + String(definitionID) }
    public init(machineID: String, projectID: String, definitionID: Int64, name: String, profile: String) {
        self.machineID = machineID; self.projectID = projectID; self.definitionID = definitionID
        self.name = name; self.profile = profile
    }
}

public struct CollaborationWorkspace: Codable, Equatable, Sendable, Identifiable {
    public let id: String
    public var name: String
    public var projects: [WorkspaceProjectReference]
    public var agents: [WorkspaceAgentReference]
    public var archived: Bool
    public let createdAt: Date
    public var updatedAt: Date
}

public enum CollaborationWorkspaceError: LocalizedError {
    case invalidConfiguration, notFound, damagedStore
    public var errorDescription: String? {
        switch self {
        case .invalidConfiguration: "工作空间配置无效，请检查名称和项目、Agent 关联。"
        case .notFound: "找不到这个工作空间，原有数据未被修改。"
        case .damagedStore: "无法读取工作空间，原有数据已保留。"
        }
    }
}

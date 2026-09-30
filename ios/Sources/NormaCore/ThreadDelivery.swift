import Foundation
import CryptoKit

public enum ThreadDeliveryError: LocalizedError {
    case unconfigured, multipleMachines, targetChanged, invalidMessage, invalidReceipt, requiresUpgrade
    public var errorDescription: String? {
        switch self {
        case .unconfigured: "请先在工作空间关联项目，消息已保留待发送。"
        case .multipleMachines: "当前阶段支持向一台 Mac 同步线程，请调整工作空间关联的项目。"
        case .targetChanged: "原操作的项目已移出工作空间，消息已保留，请核对目标。"
        case .invalidMessage: "这条消息无法传输，原文已保留；请检查目标或缩短内容后另发。"
        case .invalidReceipt: "收到的回执与原消息不符，尚未标记送达。"
        case .requiresUpgrade: "这台 Mac 暂不支持线程消息送达，请更新桌面端。"
        }
    }
}

public struct ThreadMessageOperation: Codable, Equatable, Sendable {
    public let operationID: String
    public let collaborationWorkspaceID: String
    public let threadID: String
    public let messageID: String
    public let originDeviceID: String
    public let projectIDs: [String]
    public let text: String
    enum CodingKeys: String, CodingKey {
        case operationID = "operation_id", collaborationWorkspaceID = "collaboration_workspace_id",
             threadID = "thread_id", messageID = "message_id", originDeviceID = "origin_device_id",
             projectIDs = "project_ids", text
    }

    func validate(machineID: String) throws {
        guard machineID.hasPrefix("mac-"), validHex(String(machineID.dropFirst(4))),
              [operationID, collaborationWorkspaceID, threadID, messageID, originDeviceID].allSatisfy({ UUID(uuidString: $0) != nil }),
              (1...32).contains(projectIDs.count), projectIDs.allSatisfy(validID),
              projectIDs == projectIDs.sorted(), Set(projectIDs).count == projectIDs.count,
              !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, text.utf8.count <= 64 * 1024,
              try JSONEncoder().encode(self).count <= 192 * 1024 else { throw ThreadDeliveryError.invalidMessage }
    }

    public func fingerprint(machineID: String) throws -> String {
        try validate(machineID: machineID)
        var bytes = Data("Norma thread message v1\0".utf8)
        func count(_ value: Int) { var value = UInt32(value).bigEndian; withUnsafeBytes(of: &value) { bytes.append(contentsOf: $0) } }
        func string(_ value: String) { let data = Data(value.utf8); count(data.count); bytes.append(data) }
        for value in [machineID, operationID, collaborationWorkspaceID, threadID, messageID, originDeviceID] { string(value) }
        count(projectIDs.count)
        projectIDs.forEach(string)
        string(text)
        return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
    }
}

public struct ThreadMessageReceipt: Codable, Equatable, Sendable {
    public var sourceMacID: String
    public var collaborationWorkspaceID: String
    public var threadID: String
    public var operationID: String
    public var messageID: String
    public var originDeviceID: String
    public var fingerprint: String
    public var receivedAtUnix: UInt64
    public var eventID: String
    public var cursor: UInt64
    public var state: String
    enum CodingKeys: String, CodingKey {
        case sourceMacID = "source_mac_id", collaborationWorkspaceID = "collaboration_workspace_id",
             threadID = "thread_id", operationID = "operation_id", messageID = "message_id", originDeviceID = "origin_device_id",
             fingerprint, receivedAtUnix = "received_at_unix", eventID = "event_id", cursor, state
    }

    func validate(message: ThreadMessageOperation, machineID: String) throws {
        guard sourceMacID == machineID, collaborationWorkspaceID == message.collaborationWorkspaceID,
              threadID == message.threadID, operationID == message.operationID, messageID == message.messageID,
              originDeviceID == message.originDeviceID, state == "received", receivedAtUnix > 0,
              cursor > 0, cursor <= Int64.max, eventID == machineID + ":" + String(cursor),
              fingerprint == (try message.fingerprint(machineID: machineID)) else { throw ThreadDeliveryError.invalidReceipt }
    }
}

public struct ThreadOutboxOperation: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let messageID: String
    public internal(set) var machineID: String?
    public internal(set) var message: ThreadMessageOperation?
    public internal(set) var receipt: ThreadMessageReceipt?
}

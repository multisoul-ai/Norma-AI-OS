import Foundation

/// Mac-owned tool observation. Deliberately contains no executable command or submit method.
public struct RealtimeOperation: Codable, Sendable, Equatable {
    public let sequence: UInt64
    public let deviceID: String
    public let callID: String
    public let turnID: String
    public let workspaceID: String
    public let operationID: String
    public var result: Result?
    public struct Result: Codable, Sendable, Equatable {
        public let status: String
        public let message: String?
        public let taskID: UInt64?
        enum CodingKeys: String, CodingKey { case status, message, taskID = "task_id" }
    }
    enum CodingKeys: String, CodingKey {
        case sequence, result, deviceID = "device_id", callID = "call_id", turnID = "turn_id"
        case workspaceID = "workspace_id", operationID = "operation_id"
    }
    func validate() throws {
        guard sequence > 0, sequence <= Int64.max, [deviceID, callID, turnID, workspaceID, operationID].allSatisfy(validID),
              RealtimeBinding.validCallID(callID), operationID.hasPrefix("rt-") else { throw MobileError.invalidResponse }
        if let result {
            guard ["accepted", "succeeded", "rejected", "unknown"].contains(result.status),
                  (result.message?.utf8.count ?? 0) <= 4096,
                  result.taskID.map({ $0 > 0 && $0 <= Int64.max }) ?? true else { throw MobileError.invalidResponse }
        }
    }
}

public struct RealtimeOperationObservation: Codable, Sendable, Equatable, Identifiable {
    public let machineID: String
    public var operation: RealtimeOperation
    public var targetName: String?
    public var id: String { machineID + "/" + operation.deviceID + "/" + operation.operationID }
    public var unresolved: Bool { operation.result == nil || operation.result?.status == "unknown" }
    public var label: String {
        switch operation.result?.status {
        case "accepted": "任务已接收"
        case "succeeded": "工具结果已核对"
        case "rejected": "操作未获执行"
        default: "执行结果待核对"
        }
    }
    public init(machineID: String, operation: RealtimeOperation) throws {
        self.machineID = machineID; self.operation = operation
        try validate()
    }
    func validate() throws {
        guard machineID.hasPrefix("mac-"), validHex(String(machineID.dropFirst(4))) else { throw MobileError.invalidResponse }
        try operation.validate()
        guard (targetName?.utf8.count ?? 0) <= 1024 else { throw MobileError.invalidResponse }
    }
    public func applying(receipt data: Data) throws -> Self {
        struct Receipt: Decodable {
            let device_id: String, operation_id: String, workspace_id: String, route_id: String, state: String
            let result: RealtimeOperation.Result?
            let error: String?
        }
        let receipt = try JSONDecoder().decode(Receipt.self, from: data)
        guard receipt.device_id == operation.deviceID, receipt.operation_id == operation.operationID,
              receipt.workspace_id == operation.workspaceID, validID(receipt.route_id) else { throw MobileError.invalidResponse }
        var next = self
        switch receipt.state {
        case "accepted":
            guard receipt.result == nil, receipt.error == nil else { throw MobileError.invalidResponse }
        case "succeeded":
            guard let result = receipt.result, receipt.error == nil else { throw MobileError.invalidResponse }
            next.operation.result = result
        case "rejected":
            guard receipt.result == nil, let error = receipt.error, validID(error), error != "outcome_unknown" else { throw MobileError.invalidResponse }
            next.operation.result = .init(status: "rejected", message: error, taskID: nil)
        case "outcome_unknown":
            guard receipt.result == nil, receipt.error == "outcome_unknown" else { throw MobileError.invalidResponse }
            next.operation.result = .init(status: "unknown", message: "请核对原回执", taskID: nil)
        default: throw MobileError.invalidResponse
        }
        try next.validate()
        return next
    }
}

public actor RealtimeOperationStore {
    private let url: URL
    public init(url: URL) { self.url = url }
    public func load() throws -> [RealtimeOperationObservation] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        let data = try Data(contentsOf: url)
        guard data.count <= 1024 * 1024 else { throw MobileError.invalidResponse }
        let records = try JSONDecoder().decode([RealtimeOperationObservation].self, from: data)
        guard records.count <= 128, Set(records.map(\.id)).count == records.count else { throw MobileError.invalidResponse }
        try records.forEach { try $0.validate() }
        return records
    }
    public func save(_ record: RealtimeOperationObservation) throws {
        try record.validate()
        var records = try load()
        if let index = records.firstIndex(where: { $0.id == record.id }) {
            if records[index] == record { return }
            var old = records[index].operation, next = record.operation
            old.result = nil; next.result = nil
            guard old == next else { throw MobileError.invalidResponse }
            if !records[index].unresolved && record.unresolved { return }
            if !records[index].unresolved && records[index].operation.result != record.operation.result {
                throw MobileError.invalidResponse
            }
            records[index] = record
        } else {
            if records.count == 128, let resolved = records.firstIndex(where: { !$0.unresolved }) { records.remove(at: resolved) }
            guard records.count < 128 else { throw MobileError.invalidResponse } // Never evict an unconfirmed operation.
            records.append(record)
        }
        let data = try JSONEncoder().encode(records)
        // 128 observations × (4 KiB message + bounded identities) fit within 1 MiB.
        guard data.count <= 1024 * 1024 else { throw MobileError.invalidResponse }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        #if os(iOS)
        try data.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        #else
        try data.write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        #endif
    }
}

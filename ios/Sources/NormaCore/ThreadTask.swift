import Foundation
import CryptoKit

public enum ThreadTaskError: LocalizedError {
    case invalidTask, invalidReceipt, sourceMissing, projectOutsideThread, requiresUpgrade, resultsRequireUpgrade, acceptanceRequiresUpgrade, terminalRequiresUpgrade, followupRequiresUpgrade, receiptMissing
    public var errorDescription: String? {
        switch self {
        case .invalidTask: "任务信息不完整或内容过长，请检查后重新发送。"
        case .invalidReceipt: "派工回执与原任务不符，尚未确认执行。"
        case .sourceMissing: "来源消息尚未送达 Mac，请先完成消息同步。"
        case .projectOutsideThread: "任务项目不在原消息的工作范围，请核对目标。"
        case .requiresUpgrade: "这台 Mac 暂不支持工作线程派工，请更新桌面端。"
        case .resultsRequireUpgrade: "任务已交给 Agent，请更新 Mac 桌面端以同步执行结果。"
        case .acceptanceRequiresUpgrade: "这台 Mac 暂不支持工作线程成果验收，请更新桌面端。"
        case .terminalRequiresUpgrade: "这台 Mac 暂不支持查看工作线程的终端，请更新桌面端。"
        case .followupRequiresUpgrade: "这台 Mac 暂不支持原任务追问，请更新桌面端。"
        case .receiptMissing: "Mac 暂时无法确认已接收的任务，已保留原记录，不会重复派工。"
        }
    }
}

/// Persist this exact value before sending. A refreshed route never changes this task's identity.
public struct ThreadTaskOperation: Codable, Equatable, Sendable {
    public let operationID: String
    public let collaborationWorkspaceID: String
    public let threadID: String
    public let sourceOperationID: String
    public let sourceMessageID: String
    public let projectID: String
    public let agentDefinitionID: Int64
    public let task: String
    public let contextVersion: UInt64
    public let context: String
    public var revisionOf: ThreadTaskRevision? = nil
    enum CodingKeys: String, CodingKey {
        case operationID = "operation_id", collaborationWorkspaceID = "collaboration_workspace_id", threadID = "thread_id",
             sourceOperationID = "source_operation_id", sourceMessageID = "source_message_id", projectID = "project_id",
             agentDefinitionID = "agent_definition_id", task, contextVersion = "context_version", context, revisionOf = "revision_of"
    }
    public init(operationID: String, collaborationWorkspaceID: String, threadID: String, sourceOperationID: String,
                sourceMessageID: String, projectID: String, agentDefinitionID: Int64, task: String, contextVersion: UInt64, context: String) {
        self.operationID = operationID; self.collaborationWorkspaceID = collaborationWorkspaceID; self.threadID = threadID
        self.sourceOperationID = sourceOperationID; self.sourceMessageID = sourceMessageID; self.projectID = projectID
        self.agentDefinitionID = agentDefinitionID; self.task = task; self.contextVersion = contextVersion; self.context = context
    }

    func validate(machineID: String) throws {
        if let revisionOf {
            guard UUID(uuidString: revisionOf.operationID) != nil, revisionOf.operationID != operationID,
                  revisionOf.taskRef.utf8.count <= 160, !revisionOf.taskRef.isEmpty,
                  validHex(revisionOf.fingerprint) else { throw ThreadTaskError.invalidTask }
        }
        guard machineID.hasPrefix("mac-"), validHex(String(machineID.dropFirst(4))),
              [operationID, collaborationWorkspaceID, threadID, sourceOperationID, sourceMessageID].allSatisfy({ UUID(uuidString: $0) != nil }),
              operationID != sourceOperationID, validID(projectID), agentDefinitionID > 0,
              contextVersion > 0, contextVersion <= Int64.max, !task.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              task.utf8.count + context.utf8.count <= 32 * 1024,
              (task + context).unicodeScalars.allSatisfy({ $0.value == 9 || $0.value == 10 || ($0.value >= 32 && !(127...159).contains($0.value)) }),
              try JSONEncoder().encode(self).count <= 64 * 1024 else { throw ThreadTaskError.invalidTask }
    }

    public func fingerprint(machineID: String) throws -> String {
        try validate(machineID: machineID)
        var bytes = Data("Norma thread task v1\0".utf8)
        func string(_ text: String) {
            let data = Data(text.utf8); var length = UInt32(data.count).bigEndian
            withUnsafeBytes(of: &length) { bytes.append(contentsOf: $0) }; bytes.append(data)
        }
        for value in [machineID, operationID, collaborationWorkspaceID, threadID, sourceOperationID, sourceMessageID, projectID] { string(value) }
        var definition = UInt64(agentDefinitionID).bigEndian, version = contextVersion.bigEndian
        withUnsafeBytes(of: &definition) { bytes.append(contentsOf: $0) }
        withUnsafeBytes(of: &version) { bytes.append(contentsOf: $0) }
        string(task); string(context)
        if let revisionOf {
            string("Norma thread revision v1"); string(revisionOf.operationID)
            string(revisionOf.taskRef); string(revisionOf.fingerprint)
        }
        return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
    }
}

public struct ThreadTaskRevision: Codable, Equatable, Sendable {
    public let operationID: String
    public let taskRef: String
    public let fingerprint: String
    enum CodingKeys: String, CodingKey { case operationID = "operation_id", taskRef = "task_ref", fingerprint }
}

public struct ThreadTaskReceipt: Codable, Equatable, Sendable {
    public let binding: Binding
    public let fingerprint: String
    public let receipt: Receipt
    public var execution: ThreadExecutionEvent? = nil
    /// Derived from the durable Mac receipt, so a restart cannot hide the refusal reason.
    public var rejectionDescription: String? {
        guard receipt.state == "rejected" else { return nil }
        return switch receipt.error {
        case "agent_not_found": "这位 Agent 已不在 Mac 项目中，请检查工作空间的参与成员。"
        case "agent_disabled": "这位 Agent 已在 Mac 上停用，请在电脑上检查成员设置。"
        case "route_expired", "session_changed": MobileError.sessionChanged.errorDescription
        case "unauthorized": MobileError.unauthorized.errorDescription
        case "grant_changed": MobileError.grantChanged.errorDescription
        case "busy": MobileError.busy.errorDescription
        case "unavailable": MobileError.unavailable.errorDescription
        case "expired": "派工请求已过期，Mac 没有开始这项任务。"
        default: "Mac 拒绝了这项任务，请在电脑上核对。"
        }
    }
    public struct Binding: Codable, Equatable, Sendable {
        public let sourceMacID: String
        public let collaborationWorkspaceID: String
        public let threadID: String
        public let sourceOperationID: String
        public let sourceMessageID: String
        public let payload: String
        enum CodingKeys: String, CodingKey {
            case sourceMacID = "source_mac_id", collaborationWorkspaceID = "collaboration_workspace_id", threadID = "thread_id",
                 sourceOperationID = "source_operation_id", sourceMessageID = "source_message_id", payload
        }
    }
    public struct Receipt: Codable, Equatable, Sendable {
        public let deviceID: String
        public let operationID: String
        public let workspaceID: String
        public let routeID: String
        public let state: String
        public let result: Result?
        public let error: String?
        public struct Result: Codable, Equatable, Sendable {
            public let status: String
            public let message: String?
            public let nodeID: UInt64?
            public let taskID: UInt64?
            public let taskRef: String?
            enum CodingKeys: String, CodingKey { case status, message, nodeID = "node_id", taskID = "task_id", taskRef = "task_ref" }
        }
        enum CodingKeys: String, CodingKey {
            case deviceID = "device_id", operationID = "operation_id", workspaceID = "workspace_id", routeID = "route_id", state, result, error
        }
    }
    func validate(task: ThreadTaskOperation, machineID: String, deviceID: String) throws {
        guard binding.sourceMacID == machineID, binding.collaborationWorkspaceID == task.collaborationWorkspaceID,
              binding.threadID == task.threadID, binding.sourceOperationID == task.sourceOperationID,
              binding.sourceMessageID == task.sourceMessageID, binding.payload.utf8.count <= 64 * 1024,
              try JSONDecoder().decode(ThreadTaskOperation.self, from: Data(binding.payload.utf8)) == task,
              fingerprint == (try task.fingerprint(machineID: machineID)),
              receipt.deviceID == deviceID, receipt.operationID == task.operationID,
              receipt.workspaceID == task.projectID, validID(receipt.routeID) else { throw ThreadTaskError.invalidReceipt }
        switch receipt.state {
        case "accepted":
            guard receipt.result == nil, receipt.error == nil else { throw ThreadTaskError.invalidReceipt }
        case "succeeded":
            guard receipt.error == nil, let result = receipt.result, result.status == "accepted",
                  let taskID = result.taskID, taskID > 0, taskID <= Int64.max,
                  let taskRef = result.taskRef, taskRef.utf8.count <= 160,
                  let nodeID = result.nodeID, nodeID > 0, nodeID <= Int64.max,
                  (result.message?.utf8.count ?? 0) <= 4096 else { throw ThreadTaskError.invalidReceipt }
            let identity = taskRef.split(separator: ":", omittingEmptySubsequences: false)
            guard identity.count == 2, validID(String(identity[0])), identity[1] == String(taskID) else { throw ThreadTaskError.invalidReceipt }
        case "rejected":
            guard receipt.result == nil, let error = receipt.error, validID(error), error != "outcome_unknown" else { throw ThreadTaskError.invalidReceipt }
        case "outcome_unknown":
            guard receipt.result == nil, receipt.error == "outcome_unknown" else { throw ThreadTaskError.invalidReceipt }
        default: throw ThreadTaskError.invalidReceipt
        }
        try execution?.validate(task: task, machineID: machineID, receipt: receipt)
    }
}

extension CollaborationRelayConnection {
    /// The caller saves the immutable task first; a lost response is recovered by threadTaskReceipt.
    public func submitThreadTask(_ task: ThreadTaskOperation, routeID: String, grantRevision: UInt64) async throws -> ThreadTaskReceipt {
        try requireThreadTask(task)
        guard validID(routeID), grantRevision > 0 else { throw ThreadTaskError.invalidTask }
        let value = try JSONSerialization.jsonObject(with: JSONEncoder().encode(task))
        let reply = try await exchange(type: "thread_task", fields: ["task": value, "route_id": routeID, "grant_revision": grantRevision])
        guard let receipt = try validatedTaskReply(reply, task: task) else { close(); throw ThreadTaskError.invalidReceipt }
        return receipt
    }

    public func threadTaskReceipt(for task: ThreadTaskOperation) async throws -> ThreadTaskReceipt? {
        try requireThreadTask(task)
        let reply = try await exchange(type: "thread_task_receipt", fields: ["collaboration_workspace_id": task.collaborationWorkspaceID,
            "thread_id": task.threadID, "operation_id": task.operationID])
        return try validatedTaskReply(reply, task: task)
    }

    private func requireThreadTask(_ task: ThreadTaskOperation) throws {
        guard capabilities.contains("thread_tasks_v1") else { throw ThreadTaskError.requiresUpgrade }
        if task.revisionOf != nil && !capabilities.contains("thread_task_revision_v1") { throw ThreadTaskError.requiresUpgrade }
        try task.validate(machineID: machineID)
    }
    private func validatedTaskReply(_ reply: WireReply, task: ThreadTaskOperation) throws -> ThreadTaskReceipt? {
        do {
            guard reply.result?.status == "thread_task_receipt", let grant else { throw ThreadTaskError.invalidReceipt }
            if let receipt = reply.result?.threadTask { try receipt.validate(task: task, machineID: machineID, deviceID: grant.deviceID) }
            return reply.result?.threadTask
        } catch { close(); throw error }
    }
}

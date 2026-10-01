import XCTest
@testable import NormaCore

final class ThreadFollowupTests: XCTestCase {
    /// 追问复用原会话，但排队与后续回合回复有独立身份。
    /// 数据构造：原任务 runtime-a:7/节点2/original-turn；追问操作1与来源操作4各自独立。
    /// 指纹为 Python hashlib/大端长度前缀算出的固定跨语言向量，正文10字×3 =30 UTF-8字节。
    /// 执行过程：验证排队回执 → 编解码恢复 → 新回合完成 → 注入旧回合和错误会话。
    /// 预期结果：排队无回复；新回复保留；旧任务成果及不同会话不得进入追问。
    func testFollowupValidatesItsOwnReceiptAndTurn() throws {
        let operation = try decode(ThreadFollowupOperation.self, payload)
        XCTAssertEqual(try operation.fingerprint(machineID: mac), "882846db6a611dedfbd8151d93501bf58f218dadb38875b09c9a739a4eb1fbc5", "与独立Rust/Python协议向量相同")
        let parent = try makeParent()
        let source = ThreadMessageOperation(operationID: id(4), collaborationWorkspaceID: id(2), threadID: id(3),
            messageID: id(5), originDeviceID: id(9), projectIDs: ["project-a"], text: "为什么这样修改首屏？")
        XCTAssertEqual(try ThreadFollowupOperation(operationID: id(1), parent: parent, source: source), operation,
                       "公开构造必须沿用新来源、原任务及完整task_ref")
        let originalKey = try decode(MobileTurnOutcome.Key.self, key("original-turn"))
        let queued = try decode(ThreadFollowupReceipt.self, response(sent: false))
        try queued.validate(operation: operation, parent: parent, originalKey: originalKey)
        XCTAssertNil(queued.outcome, "队列签收不能显示Agent回复")
        XCTAssertNil(queued.delivery, "父任务成果不得充当追问成果")
        let sent = try decode(ThreadFollowupReceipt.self, response(sent: true))
        try sent.validate(operation: operation, parent: parent, originalKey: originalKey)
        let restored = try JSONDecoder().decode(ThreadFollowupReceipt.self, from: JSONEncoder().encode(sent))
        XCTAssertEqual(restored.outcome?.summary, "保留当前目标。", "持久编解码必须保留追问回复正文")
        XCTAssertEqual(restored.outcome?.key.turnID, "followup-turn", "本次回复属于独立新回合")
        var stale = response(sent: true)
        stale["outcome"] = ["key": key("original-turn"), "state": "completed", "summary": "旧结果"]
        XCTAssertThrowsError(try decode(ThreadFollowupReceipt.self, stale).validate(operation: operation, parent: parent, originalKey: originalKey), "旧回合不能覆盖追问")
        var changedSession = response(sent: true)
        var queue = try XCTUnwrap(changedSession["queued_message"] as? [String: Any])
        queue["session_id"] = "another-session"; changedSession["queued_message"] = queue
        XCTAssertThrowsError(try decode(ThreadFollowupReceipt.self, changedSession).validate(operation: operation, parent: parent, originalKey: originalKey), "相同节点的新session不可被当作原会话")
        var forged = response(sent: false)
        var binding = try XCTUnwrap(forged["binding"] as? [String: Any])
        binding["thread_id"] = id(9); forged["binding"] = binding
        XCTAssertThrowsError(try decode(ThreadFollowupReceipt.self, forged).validate(operation: operation, parent: parent, originalKey: originalKey), "不同线程的回执不得持久化")
    }

    private let mac = "mac-" + String(repeating: "a", count: 64)
    private func id(_ value: Int) -> String { String(format: "00000000-0000-4000-8000-%012d", value) }
    private var payload: [String: Any] {
        ["operation_id": id(1), "collaboration_workspace_id": id(2), "thread_id": id(3),
         "source_operation_id": id(4), "source_message_id": id(5), "parent_operation_id": id(6),
         "task_ref": "runtime-a:7", "text": "为什么这样修改首屏？"]
    }
    private func decode<T: Decodable>(_ type: T.Type, _ value: [String: Any]) throws -> T {
        try JSONDecoder().decode(type, from: JSONSerialization.data(withJSONObject: value))
    }
    private func key(_ turn: String) -> [String: Any] {
        ["node_id": 2, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": turn]
    }
    private func makeParent() throws -> ThreadTaskOutboxOperation {
        let task = ThreadTaskOperation(operationID: id(6), collaborationWorkspaceID: id(2), threadID: id(3),
            sourceOperationID: id(7), sourceMessageID: id(8), projectID: "project-a", agentDefinitionID: 2,
            task: "修改首屏", contextVersion: 1, context: "")
        var parent = ThreadTaskOutboxOperation(machineID: mac, task: task)
        parent.deviceID = "phone-a"
        parent.receipt = try .init(binding: .init(sourceMacID: mac, collaborationWorkspaceID: id(2), threadID: id(3),
            sourceOperationID: id(7), sourceMessageID: id(8), payload: String(decoding: JSONEncoder().encode(task), as: UTF8.self)),
            fingerprint: task.fingerprint(machineID: mac), receipt: .init(deviceID: "phone-a", operationID: id(6),
                workspaceID: "project-a", routeID: "route-a", state: "succeeded",
                result: .init(status: "accepted", message: nil, nodeID: 2, taskID: 7, taskRef: "runtime-a:7"), error: nil))
        return parent
    }
    private func response(sent: Bool) -> [String: Any] {
        let result: [String: Any] = ["node_id": 2, "run_id": "run-a", "session_id": "session-a", "turn_id": "followup-turn"]
        var queue: [String: Any] = ["device_id": "phone-a", "operation_id": id(1), "workspace_id": "project-a",
            "route_id": "route-a", "node_id": 2, "profile": "Codex", "run_id": "run-a", "session_id": "session-a",
            "text": payload["text"]!, "state": sent ? "sent" : "queued"]
        if sent { queue["result"] = result }
        var response: [String: Any] = ["original_key": key("original-turn"), "binding": ["source_mac_id": mac, "collaboration_workspace_id": id(2),
            "thread_id": id(3), "source_operation_id": id(4), "source_message_id": id(5),
            "payload": String(decoding: try! JSONSerialization.data(withJSONObject: payload), as: UTF8.self)],
            "fingerprint": "882846db6a611dedfbd8151d93501bf58f218dadb38875b09c9a739a4eb1fbc5",
            "receipt": ["device_id": "phone-a", "operation_id": id(1), "workspace_id": "project-a", "route_id": "route-a",
                "state": "succeeded", "result": ["node_id": 2, "run_id": "run-a", "session_id": "session-a", "queue_id": id(1), "status": "queued"]],
            "queued_message": queue]
        if sent { response["outcome"] = ["key": key("followup-turn"), "state": "completed", "summary": "保留当前目标。"] }
        return response
    }
}

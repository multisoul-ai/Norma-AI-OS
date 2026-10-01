import Foundation
import XCTest
@testable import NormaCore

final class ThreadTaskTests: XCTestCase {
    /// 派工指纹独立于尝试路由，回执只绑定原任务及原线程；派工接收不能冒充任务完成。
    /// 数据构造：5 个固定 UUID；Agent 定义 2、上下文版本 1；SHA256 期望由独立 Node crypto 按协议生成。
    /// 执行过程：编码任务 → 校验已接收/派工回执 → 替换线程、来源消息、正文、任务身份分别验证。
    /// 预期结果：合法回执保留任务 7；篡改均拒绝；无 task_id 的成功和 accepted 携带结果均拒绝。
    func testTaskReceiptRejectsWrongScopeAndFalseExecution() throws {
        let task = try JSONDecoder().decode(ThreadTaskOperation.self, from: JSONSerialization.data(withJSONObject: taskJSON))
        let fingerprint = try task.fingerprint(machineID: machine)
        XCTAssertEqual(fingerprint, "4e7836e753baa05af065bc708dd5b2d302dd67baf378d6ea786786f33e0babfa", "Swift 与协议的独立固定向量必须一致")
        let payload = String(decoding: try JSONSerialization.data(withJSONObject: taskJSON), as: UTF8.self)
        let binding: [String: Any] = ["source_mac_id": machine, "collaboration_workspace_id": id(2), "thread_id": id(3),
            "source_operation_id": id(4), "source_message_id": id(5), "payload": payload]
        var receipt: [String: Any] = ["device_id": "phone", "operation_id": id(1), "workspace_id": "project-a",
            "route_id": "original-route", "state": "accepted"]
        func decode(_ receipt: [String: Any], binding: [String: Any]) throws -> ThreadTaskReceipt {
            try JSONDecoder().decode(ThreadTaskReceipt.self, from: JSONSerialization.data(withJSONObject:
                ["binding": binding, "fingerprint": fingerprint, "receipt": receipt]))
        }
        XCTAssertNoThrow(try decode(receipt, binding: binding).validate(task: task, machineID: machine, deviceID: "phone"), "接收回执不要求任务已经开始")
        receipt["state"] = "succeeded"
        receipt["result"] = ["status": "accepted", "node_id": 2, "task_id": 7, "task_ref": "runtime-a:7"]
        let saved = try decode(receipt, binding: binding)
        XCTAssertNoThrow(try saved.validate(task: task, machineID: machine, deviceID: "phone"), "合法派工回执可恢复")
        XCTAssertEqual(saved.receipt.result?.taskID, 7, "必须保留具体任务，不能用另一任务结果替代")
        XCTAssertEqual(saved.receipt.result?.taskRef, "runtime-a:7", "保存跨重启可查询的原任务引用")
        XCTAssertEqual(saved.receipt.result?.status, "accepted", "派工成功仍只是 Agent 接收任务")
        var wrong = binding
        wrong["thread_id"] = id(6)
        XCTAssertThrowsError(try decode(receipt, binding: wrong).validate(task: task, machineID: machine, deviceID: "phone"), "另一线程不能接收结果")
        wrong = binding; wrong["source_message_id"] = id(6)
        XCTAssertThrowsError(try decode(receipt, binding: wrong).validate(task: task, machineID: machine, deviceID: "phone"), "来源消息不可替换")
        wrong = binding; wrong["payload"] = payload.replacingOccurrences(of: "修复首屏导航", with: "修改别的功能")
        XCTAssertThrowsError(try decode(receipt, binding: wrong).validate(task: task, machineID: machine, deviceID: "phone"), "原始正文必须匹配")
        receipt["operation_id"] = id(6)
        XCTAssertThrowsError(try decode(receipt, binding: binding).validate(task: task, machineID: machine, deviceID: "phone"), "另一操作不能冒充回执")
        receipt["operation_id"] = id(1); receipt["state"] = "accepted"
        XCTAssertThrowsError(try decode(receipt, binding: binding).validate(task: task, machineID: machine, deviceID: "phone"), "排队状态不能携带派工结果")
        receipt["state"] = "succeeded"; receipt["result"] = ["status": "accepted", "node_id": 2]
        XCTAssertThrowsError(try decode(receipt, binding: binding).validate(task: task, machineID: machine, deviceID: "phone"), "没有具体 task_id 不能显示已派工")
        receipt["result"] = ["status": "accepted", "node_id": 2, "task_id": 7, "task_ref": "runtime-a:8"]
        XCTAssertThrowsError(try decode(receipt, binding: binding).validate(task: task, machineID: machine, deviceID: "phone"), "持久引用不能属于另一个任务")
    }

    private let machine = "mac-" + String(repeating: "a", count: 64)
    private func id(_ n: Int) -> String { [8,4,4,4,12].map { String(repeating: String(n), count: $0) }.joined(separator: "-") }
    private var taskJSON: [String: Any] {
        ["operation_id":id(1), "collaboration_workspace_id":id(2), "thread_id":id(3), "source_operation_id":id(4),
         "source_message_id":id(5), "project_id":"project-a", "agent_definition_id":2, "task":"修复首屏导航",
         "context_version":1, "context":"目标：简化导航"]
    }
}

import XCTest
@testable import NormaCore

final class ThreadTerminalTests: XCTestCase {
    /// 手机只接收原任务、原路由和已确认回合的终端页；完成后无活动turn仍可读原最后回合。
    /// 数据构造：一个派工runtime-a:7、节点2、80列24行；24<120所以首行必须为0。
    /// 执行过程：确认原派工 → 读取合法页/完成页 → 分别篡改线程、引用、路由、回合和输入权。
    /// 预期结果：两份原页通过；所有跨任务、会话矛盾及未获得接管权的输入状态均被拒绝。
    func testTerminalPageRequiresOriginalTaskAndProviderIdentity() throws {
        let machine = "mac-" + String(repeating: "a", count: 64)
        let device = UUID().uuidString
        let task = ThreadTaskOperation(operationID: UUID().uuidString, collaborationWorkspaceID: UUID().uuidString,
            threadID: UUID().uuidString, sourceOperationID: UUID().uuidString, sourceMessageID: UUID().uuidString,
            projectID: "project-a", agentDefinitionID: 2, task: "原任务", contextVersion: 1, context: "")
        let receipt = ThreadTaskReceipt(binding: .init(sourceMacID: machine, collaborationWorkspaceID: task.collaborationWorkspaceID,
            threadID: task.threadID, sourceOperationID: task.sourceOperationID, sourceMessageID: task.sourceMessageID,
            payload: String(decoding: try JSONEncoder().encode(task), as: UTF8.self)), fingerprint: try task.fingerprint(machineID: machine),
            receipt: .init(deviceID: device, operationID: task.operationID, workspaceID: task.projectID, routeID: "route-a", state: "succeeded",
                result: .init(status: "accepted", message: nil, nodeID: 2, taskID: 7, taskRef: "runtime-a:7"), error: nil))
        try receipt.validate(task: task, machineID: machine, deviceID: device)
        let request = try ThreadTerminalRequest(task: task, receipt: receipt)
        let scope: [String: Any] = ["workspace_id": "project-a", "route_id": "route-a", "node_id": 2, "run_id": "run-a", "session_id": "session-a"]
        let key: [String: Any] = ["node_id": 2, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a"]
        let card: [String: Any] = ["node_id": 2, "profile": "Codex", "title": "任务", "mobile_control": true,
            "state": "thinking", "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a"]
        let page: [String: Any] = ["card": card, "revision": 9, "screen_signature": 17, "input_sequence": 0,
            "start_line": 0, "total_lines": 24, "cols": 80, "running": true, "input_ready": false,
            "rows": Array(repeating: "原任务输出", count: 24), "soft_wrapped": Array(repeating: false, count: 24)]
        let encodedRequest = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))
        let body: [String: Any] = ["request": encodedRequest, "scope": scope, "key": key, "page": page]
        func read(_ value: [String: Any]) throws -> ThreadTaskTerminal {
            let response = try JSONDecoder().decode(ThreadTaskTerminal.self, from: JSONSerialization.data(withJSONObject: value))
            try response.validate(for: request, task: task, receipt: receipt)
            return response
        }
        XCTAssertEqual(try read(body).page.rows.first, "原任务输出", "原任务终端正文应保留")
        var completedCard = card; completedCard["turn_id"] = NSNull(); completedCard["state"] = "done"
        var completedPage = page; completedPage["card"] = completedCard
        var completed = body; completed["page"] = completedPage
        XCTAssertNoThrow(try read(completed), "完成后仍可查看已绑定的原最后回合")
        var wrongRequest = try XCTUnwrap(encodedRequest as? [String: Any]); wrongRequest["thread_id"] = UUID().uuidString
        var bad = body; bad["request"] = wrongRequest
        XCTAssertThrowsError(try read(bad), "其他线程的响应不得进入当前页面")
        wrongRequest = try XCTUnwrap(encodedRequest as? [String: Any]); wrongRequest["task_ref"] = "runtime-b:7"; bad["request"] = wrongRequest
        XCTAssertThrowsError(try read(bad), "相同数字任务ID也不能换运行实例")
        var wrongScope = scope; wrongScope["route_id"] = "route-b"; bad = body; bad["scope"] = wrongScope
        XCTAssertThrowsError(try read(bad), "不能把新路由当原运行")
        var wrongKey = key; wrongKey["turn_id"] = "turn-b"; bad = body; bad["key"] = wrongKey
        XCTAssertThrowsError(try read(bad), "回合绑定不能与实际卡片矛盾")
        wrongKey = key; wrongKey["session_id"] = "session-b"; bad["key"] = wrongKey
        XCTAssertThrowsError(try read(bad), "会话变化不能继续展示原终端")
        var writable = page; writable["input_ready"] = true; bad = body; bad["page"] = writable
        XCTAssertThrowsError(try read(bad), "读取能力不能冒充接管输入权")
    }
}

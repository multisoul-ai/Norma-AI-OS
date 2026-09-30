import XCTest
@testable import NormaCore

final class PlanTodoTests: XCTestCase {
    private let raw = #"{"sequence":1,"plan_id":"plan-a","workspace_id":"project-a","route_id":"route-a","direction":"验证登录","created_at_unix":1,"draft":{"summary":"补测试","todos":[{"target":{"node_id":7,"profile":"Codex","title":"登录","mobile_control":true,"state":"ready","run_id":"run-a","session_id":"session-a","turn_id":null,"task_title":null},"title":"登录测试","instruction":"保留原文。"}]}}"#
    /// 决定持久化原始建议和用户选中的当前会话，成功回执必须匹配决定类型与真实任务。
    ///
    /// 数据构造：plan-a第0项/node7；拒绝回执status=declined；接受回执绑定run/session/turn。
    /// 执行过程：保存接受请求 → 重开 → 检查原文未进入可篡改命令 → 精确发送回执/拒绝回执 → 错来源。
    /// 预期结果：原文可找回；接受不声称完成；缺回合、错节点/计划、伪造completed都拒绝。
    func testTodoDecisionPersistsPreviewAndValidatesReceipt() async throws {
        let plan = try JSONDecoder().decode(ProjectPlan.self, from: Data(raw.utf8))
        let target = plan.draft.todos[0].target
        func command(_ choice: PlanTodoChoice) throws -> PendingMobileCommand {
            try PendingMobileCommand(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone-a", routeID: "route-current", revision: 2, deciding: plan, todoIndex: 0, choice: choice, operationID: "todo-op")
        }
        let accepted = try command(.accept(target: target, queue: false))
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("todo.json")
        try await PendingCommandStore(url: url).save(accepted)
        let restored = try await PendingCommandStore(url: url).load()
        XCTAssertEqual(restored, [accepted], "重开保留完整决定")
        XCTAssertEqual(restored.first?.todoPreview?.instruction, "保留原文。", "原文必须可找回")
        let request = try JSONEncoder().encode(accepted.request)
        XCTAssertFalse(String(decoding: request, as: UTF8.self).contains("保留原文"), "执行指令只能从Mac保存的计划读取")
        let receipt = #"{"device_id":"phone-a","operation_id":"todo-op","workspace_id":"project-a","route_id":"route-current","state":"succeeded","result":{"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"actual-turn"},"error":null}"#
        func read(_ text: String, for command: PendingMobileCommand) throws {
            try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(text.utf8)).validate(for: command)
        }
        try read(receipt, for: accepted)
        XCTAssertThrowsError(try read(receipt.replacingOccurrences(of: "actual-turn", with: ""), for: accepted), "没有实际回合不得声称收到")
        XCTAssertThrowsError(try read(receipt.replacingOccurrences(of: #""node_id":7"#, with: #""node_id":8"#), for: accepted), "另一卡片不能匹配")
        let rejected = try command(.reject)
        let decline = receipt.replacingOccurrences(of: #"{"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"actual-turn"}"#, with: #"{"plan_id":"plan-a","todo_index":0,"status":"declined"}"#)
        try read(decline, for: rejected)
        XCTAssertThrowsError(try read(decline, for: accepted), "接受不能收到拒绝回执")
        XCTAssertThrowsError(try read(decline.replacingOccurrences(of: "declined", with: "completed"), for: rejected), "决定成功不是任务完成")
        XCTAssertThrowsError(try read(decline.replacingOccurrences(of: "plan-a", with: "plan-b"), for: rejected), "其他计划不得认领")
    }

    /// 共享Todo进度按原计划校验，其他获授权手机的准确结果可见但不能混入重复索引。
    ///
    /// 数据构造：另一手机的第0项接受，真实回执为sent；第二份同index构造协议重复。
    /// 执行过程：解析进度 → 校验原计划/决定/回执 → 错项目 → 重复决定。
    /// 预期结果：只显示已发送而非已完成，不能展示错项目或无法唯一归属的决定。
    func testSharedProgressUsesActualDeliveryReceipt() throws {
        let plan = try JSONDecoder().decode(ProjectPlan.self, from: Data(raw.utf8))
        let target = try JSONSerialization.jsonObject(with: JSONEncoder().encode(plan.draft.todos[0].target))
        let decision: [String: Any] = ["todo_index": 0, "choice": ["type": "accept", "target": target, "queue": false], "receipt": ["device_id": "phone-b", "operation_id": "todo-b", "workspace_id": "project-a", "route_id": "route-a", "state": "succeeded", "result": ["node_id": 7, "run_id": "run-a", "session_id": "session-a", "turn_id": "actual-turn"]]]
        let rawDecision = try JSONSerialization.data(withJSONObject: decision)
        let decoded = try JSONDecoder().decode(ProjectTodoDecision.self, from: rawDecision)
        try decoded.validate(plan: plan, machineID: "mac-" + String(repeating: "a", count: 64))
        XCTAssertEqual(decoded.deliveryState, "sent", "实际发送回执只证明已发送")
        XCTAssertNotEqual(decoded.deliveryState, "completed", "无任务终态不能显示已完成")
        let duplicated = ProjectPlanProgress(plan: plan, decisions: [decoded, decoded])
        XCTAssertThrowsError(try duplicated.validate(workspaceID: plan.workspaceID, planID: plan.planID, machineID: "mac-" + String(repeating: "a", count: 64)), "重复索引的决定不能进入界面")
        var wrong = decision; var receipt = wrong["receipt"] as! [String: Any]; receipt["workspace_id"] = "project-b"; wrong["receipt"] = receipt
        let bad = try JSONDecoder().decode(ProjectTodoDecision.self, from: JSONSerialization.data(withJSONObject: wrong))
        XCTAssertThrowsError(try bad.validate(plan: plan, machineID: "mac-" + String(repeating: "a", count: 64)), "错项目回执应拒绝")
    }
    /// 确认预览使用同一卡片的当前会话，历史运行不能作为当前聊天跳转目标。
    ///
    /// 数据构造：原计划node7/run-a；新快照run-new正在执行；另一节点8和unknown快照是负例。
    /// 执行过程：计算直接发送/排队选择 → 对比原目标 → 校验发送回执所在会话 → 更换运行/窗口。
    /// 预期结果：明确选择当前快照；不接受错卡或未知状态；旧回执不能跳到重开的会话。
    func testCurrentTargetAndConversationNavigation() throws {
        let plan = try JSONDecoder().decode(ProjectPlan.self, from: Data(raw.utf8))
        let todo = plan.draft.todos[0]
        func card(_ run: String, state: String = "thinking", node: UInt64 = 7) -> CardSnapshot {
            CardSnapshot(nodeID: node, profile: "Codex", title: "登录", mobileControl: true, state: state, runID: run, sessionID: "session-a", taskTitle: nil, turnID: "current-turn")
        }
        XCTAssertEqual(todo.acceptance(for: todo.target), .accept(target: todo.target, queue: false), "就绪会话直接发送")
        let current = card("run-new")
        XCTAssertEqual(todo.acceptance(for: current), .accept(target: current, queue: true), "用户预览必须绑定当前忙碌会话并排队")
        XCTAssertNotEqual(todo.acceptance(for: current), .accept(target: todo.target, queue: true), "不能暗中使用生成计划时的旧运行")
        XCTAssertNil(todo.acceptance(for: card("run-new", node: 8)), "不能改派另一张卡片")
        XCTAssertNil(todo.acceptance(for: card("run-new", state: "unknown")), "状态未知时不能显示可确认")
        XCTAssertNil(todo.acceptance(for: nil), "卡片删除后不能确认")
        let json = #"{"todo_index":0,"choice":{"type":"accept","target":{"node_id":7,"profile":"Codex","title":"登录","mobile_control":true,"state":"ready","run_id":"run-a","session_id":"session-a"},"queue":false},"receipt":{"device_id":"phone-a","operation_id":"op-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"sent-turn"}}}"#
        let decision = try JSONDecoder().decode(ProjectTodoDecision.self, from: Data(json.utf8))
        func project(_ card: CardSnapshot, route: String = "route-a") -> ProjectSnapshot {
            ProjectSnapshot(workspaceID: "project-a", displayName: "登录", state: "open", routeID: route, cardCount: 1, cards: [card])
        }
        XCTAssertEqual(decision.conversationCard(in: project(card("run-a"))), card("run-a"), "同会话的后续回合仍可查看所在对话")
        XCTAssertNil(decision.conversationCard(in: project(current)), "新运行不能冒充实际发送任务")
        XCTAssertNil(decision.conversationCard(in: project(card("run-a"), route: "other-route")), "另一个窗口不得接管旧任务跳转")
    }

    /// Todo只接受精确发送回合的终态；入队回执、其他回合或未知状态都不能伪造完成。
    ///
    /// 数据构造：plan-a第0项已发送到actual-turn；同键completed为正例，其他turn/节点为负例。
    /// 执行过程：解码真实终态形状 → 验证计划和原回执 → 注入错回合/错节点/未发送/非法状态。
    /// 预期结果：准确完成退出待办；已送达、失败或停止仍需跟进，非终态字符串不可进入UI。
    func testTodoCompletionRequiresPersistedExactTurnEvidence() throws {
        let plan = try JSONDecoder().decode(ProjectPlan.self, from: Data(raw.utf8))
        let target = try JSONSerialization.jsonObject(with: JSONEncoder().encode(plan.draft.todos[0].target))
        let machine = "mac-" + String(repeating: "a", count: 64)
        var record: [String: Any] = ["todo_index": 0, "choice": ["type": "accept", "target": target, "queue": false], "receipt": ["device_id": "phone-a", "operation_id": "op-a", "workspace_id": "project-a", "route_id": "route-a", "state": "succeeded", "result": ["node_id": 7, "run_id": "run-a", "session_id": "session-a", "turn_id": "actual-turn"]]]
        func read(_ value: [String: Any]) throws -> ProjectTodoDecision {
            let decision = try JSONDecoder().decode(ProjectTodoDecision.self, from: JSONSerialization.data(withJSONObject: value))
            try decision.validate(plan: plan, machineID: machine); return decision
        }
        XCTAssertNil(try read(record).outcome, "只有发送回执不能伪造完成")
        XCTAssertTrue(try read(record).needsFollowUp, "已送达但没有完成证据的任务必须留在待办")
        var key: [String: Any] = ["node_id": 7, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": "actual-turn"]
        record["outcome"] = ["key": key, "state": "completed"]
        XCTAssertEqual(try read(record).outcome?.state, .completed, "准确Provider完成证据可显示")
        XCTAssertEqual(try read(record).deliveryState, "sent", "完成不能改变原始送达含义")
        XCTAssertFalse(try read(record).needsFollowUp, "准确完成应退出待办，原决定仍留在全部记录")
        record["outcome"] = ["key": key, "state": "failed"]
        XCTAssertTrue(try read(record).needsFollowUp, "失败任务仍需要用户调整方向，不能隐藏")
        record["outcome"] = ["key": key, "state": "interrupted"]
        XCTAssertTrue(try read(record).needsFollowUp, "已停止任务仍需要跟进，不能当作完成")
        key["turn_id"] = "other-turn"; record["outcome"] = ["key": key, "state": "completed"]
        XCTAssertThrowsError(try read(record), "不得借用另一回合的成功")
        key["turn_id"] = "actual-turn"; key["node_id"] = 8; record["outcome"] = ["key": key, "state": "failed"]
        XCTAssertThrowsError(try read(record), "不得把别的卡片失败贴到当前Todo")
        key["node_id"] = 7; record["outcome"] = ["key": key, "state": "approved"]
        XCTAssertThrowsError(try read(record), "成果验收不是Provider终态")
        record["outcome"] = ["key": key, "state": "completed"]
        record["receipt"] = ["device_id": "phone-a", "operation_id": "op-a", "workspace_id": "project-a", "route_id": "route-a", "state": "accepted"]
        XCTAssertThrowsError(try read(record), "尚无真实发送回执不能关联完成")
        record.removeValue(forKey: "outcome")
        record["choice"] = ["type": "reject"]
        record["receipt"] = ["device_id": "phone-a", "operation_id": "op-a", "workspace_id": "project-a", "route_id": "route-a", "state": "succeeded", "result": ["plan_id": "plan-a", "todo_index": 0, "status": "declined"]]
        XCTAssertFalse(try read(record).needsFollowUp, "明确未采纳的事项退出待办，但不能生成执行完成证据")
        XCTAssertNil(try read(record).outcome, "未采纳不等于Agent已完成任务")
    }

    /// 重新安排只能使用服务器确认未发送的准确旧决定，新请求保存前次身份且不能改写原文。
    ///
    /// 数据构造：node7的op-old被输入前拒绝，can_retry=true；sent/unknown和伪造标志为负例。
    /// 执行过程：读取资格 → 构造新确认并编码/重开 → 校验旧身份 → 禁止已送达与未知重排。
    /// 预期结果：新操作绑定原决定与当前目标；旧协议缺标志不能误开放，非法结果明确拒绝。
    func testExplicitRetryBindsPreviousDecisionAndRejectsAmbiguousDelivery() throws {
        let plan = try JSONDecoder().decode(ProjectPlan.self, from: Data(raw.utf8))
        let target = try JSONSerialization.jsonObject(with: JSONEncoder().encode(plan.draft.todos[0].target))
        let machine = "mac-" + String(repeating: "a", count: 64)
        var record: [String: Any] = ["todo_index": 0, "choice": ["type": "accept", "target": target, "queue": false], "receipt": ["device_id": "phone-a", "operation_id": "op-old", "workspace_id": "project-a", "route_id": "route-a", "state": "rejected", "error": "card_changed"], "can_retry": true]
        func read(_ value: [String: Any]) throws -> ProjectTodoDecision {
            let decoded = try JSONDecoder().decode(ProjectTodoDecision.self, from: JSONSerialization.data(withJSONObject: value))
            try decoded.validate(plan: plan, machineID: machine); return decoded
        }
        let old = try read(record)
        XCTAssertTrue(old.canRetry, "明确未发送才显示重新安排")
        let pending = try PendingMobileCommand(machineID: machine, deviceID: "phone-b", routeID: "current-route", revision: 2, deciding: plan, todoIndex: 0, choice: .accept(target: plan.draft.todos[0].target, queue: false), retryOf: old.reference, operationID: "op-new")
        let restored = try JSONDecoder().decode(PendingMobileCommand.self, from: JSONEncoder().encode(pending))
        XCTAssertEqual(restored, pending, "新确认先保存在手机，恢复不能丢失旧决定身份")
        let wire = try JSONSerialization.jsonObject(with: JSONEncoder().encode(pending.request)) as! [String: Any]
        let command = wire["command"] as! [String: Any]
        XCTAssertEqual((command["retry_of"] as? [String: String])?["operation_id"], "op-old", "新请求必须条件绑定旧决定")
        XCTAssertFalse(String(decoding: try JSONEncoder().encode(pending.request), as: UTF8.self).contains("保留原文"), "重排仍只能使用Mac保存的指令")
        var legacy = record; legacy.removeValue(forKey: "can_retry")
        XCTAssertFalse(try read(legacy).canRetry, "缺少服务器证明不允许重排")
        record["receipt"] = ["device_id": "phone-a", "operation_id": "op-old", "workspace_id": "project-a", "route_id": "route-a", "state": "outcome_unknown", "error": "outcome_unknown"]
        XCTAssertThrowsError(try read(record), "未知结果不能接受可重排标志")
        record["receipt"] = ["device_id": "phone-a", "operation_id": "op-old", "workspace_id": "project-a", "route_id": "route-a", "state": "succeeded", "result": ["node_id": 7, "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-real"]]
        XCTAssertThrowsError(try read(record), "真实送达也不能重排")
        XCTAssertThrowsError(try PendingMobileCommand(machineID: machine, deviceID: "phone-b", routeID: "route-a", revision: 2, deciding: plan, todoIndex: 0, choice: .reject, retryOf: old.reference), "重排必须是新的明确执行确认")
    }

}

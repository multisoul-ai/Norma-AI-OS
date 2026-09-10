import XCTest
@testable import NormaCore

final class TaskDeliveryTests: XCTestCase {
    /// 项目级成果允许不同卡片，仍拒绝另一项目、窗口、错范围和重复分页。
    ///
    /// 数据构造：20项=一页上限，序号21至2交替属于Codex7/Claude8；尾页序号1，SHA256指纹64位。
    /// 执行过程：解码项目页并校验 → 旧游标尾页 → 改成单卡范围/另一项目/旧窗口/错误游标/重复序号。
    /// 预期结果：项目页包含两卡且游标连续；单卡旧接口仍独立验证，越权与错序不能展示为有效历史。
    func testProjectHistoryKeepsScopeAndCursorAcrossCards() throws {
        func item(_ sequence: Int) -> [String: Any] {
            let node = sequence % 2 == 1 ? 7 : 8
            return ["sequence": sequence, "workspace_id": "project", "fingerprint": String(format: "%064x", sequence),
                "saved_at_unix": 100, "status": "available", "file_count": 0,
                "outcome": ["state": "completed", "key": ["node_id": node, "profile": node == 7 ? "Codex" : "Claude",
                    "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-\(sequence)"]]]
        }
        func decode(_ value: [String: Any]) throws -> TaskDeliveryPage {
            try JSONDecoder().decode(TaskDeliveryPage.self, from: JSONSerialization.data(withJSONObject: value))
        }
        let scope = ProjectReviewRequest(workspaceID: "project", routeID: "route")
        let items = (2...21).reversed().map(item)
        var page: [String: Any] = ["request": ["workspace_id": "project", "route_id": "route"],
                                  "items": items, "next_cursor": 2]
        let valid = try decode(page)
        try valid.validate(for: scope, nodeID: nil, before: nil)
        XCTAssertNil(valid.nodeID, "项目范围不能被解码成具体卡片")
        XCTAssertEqual(valid.items[0].outcome.key.nodeID, 7, "项目页必须保留第一张卡")
        XCTAssertEqual(valid.items[1].outcome.key.nodeID, 8, "项目页必须保留第二张卡")
        XCTAssertEqual(valid.nextCursor, 2, "满页游标是最后一项序号")
        XCTAssertThrowsError(try valid.validate(for: scope, nodeID: 7, before: nil), "单卡请求不能接受项目级响应")
        var tail = page; tail["items"] = [item(1)]; tail.removeValue(forKey: "next_cursor")
        let last = try decode(tail)
        try last.validate(for: scope, nodeID: nil, before: valid.nextCursor)
        XCTAssertEqual(last.items.first?.sequence, 1, "尾页只包含旧游标之前的成果")
        XCTAssertNil(last.nextCursor, "短尾页不得继续伪造下一页")
        page["node_id"] = 7
        XCTAssertThrowsError(try decode(page).validate(for: scope, nodeID: nil, before: nil), "项目请求不能接受服务端缩成单卡的结果")
        XCTAssertThrowsError(try decode(page).validate(for: scope, nodeID: 7, before: nil), "单卡范围不能夹带另一卡")
        page.removeValue(forKey: "node_id")
        var foreign = items[0]; foreign["workspace_id"] = "other"
        page["items"] = [foreign] + Array(items.dropFirst())
        XCTAssertThrowsError(try decode(page).validate(for: scope, nodeID: nil, before: nil), "项目列表仍需逐项核对项目身份")
        page["items"] = items; page["request"] = ["workspace_id": "project", "route_id": "old"]
        XCTAssertThrowsError(try decode(page).validate(for: scope, nodeID: nil, before: nil), "旧窗口成果不能成为当前结果")
        page["request"] = ["workspace_id": "project", "route_id": "route"]; page["next_cursor"] = 1
        XCTAssertThrowsError(try decode(page).validate(for: scope, nodeID: nil, before: nil), "游标不能跳过尚未展示的成果")
        page["next_cursor"] = 2; page["items"] = [items[0], items[0]] + Array(items.dropFirst(2))
        XCTAssertThrowsError(try decode(page).validate(for: scope, nodeID: nil, before: nil), "重复成果必须拒绝")
        XCTAssertThrowsError(try last.validate(for: scope, nodeID: nil, before: 1), "翻页不能重复上一页末项")
    }

    /// 修改反馈保留原始意见，成功回执必须同时证明原成果和原会话中的新回合。
    ///
    /// 数据构造：node7/run-a/session-a/old，64位a指纹，意见含首尾空格；执行回执turn为new。
    /// 执行过程：解码/磁盘编码往返 → 精确回执通过 → 错成果/旧turn/错session/缺少原成果 → 错目标命令。
    /// 预期结果：正文不被trim；仅原会话新回合通过，无法把反馈当普通消息发到重开卡片。
    @MainActor func testFeedbackKeepsOriginalOpinionAndCannotTargetAReopenedSession() async throws {
        func decode<T: Decodable>(_ value: Any, as: T.Type) throws -> T {
            try JSONDecoder().decode(T.self, from: JSONSerialization.data(withJSONObject: value))
        }
        let key: [String: Any] = ["node_id": 7, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": "old"]
        let fingerprint = String(repeating: "a", count: 64)
        let text = "  改成黑色\n保留输入框  "
        var card: [String: Any] = ["node_id": 7, "profile": "Codex", "title": "Agent", "state": "done", "mobile_control": true, "run_id": "run-a", "session_id": "session-a"]
        var command: [String: Any] = ["type": "feedback_delivery", "expected": card, "feedback": ["key": key, "fingerprint": fingerprint, "text": text]]
        func pending(_ command: [String: Any]) throws -> PendingMobileCommand {
            try decode(["machineID": "mac-" + fingerprint, "deviceID": "phone", "request": ["operation_id": "feedback", "workspace_id": "project", "route_id": "route", "grant_revision": 1, "command": command]], as: PendingMobileCommand.self)
        }
        let original = try pending(command)
        try original.validate()
        XCTAssertEqual(try JSONDecoder().decode(PendingMobileCommand.self, from: JSONEncoder().encode(original)), original, "持久化必须保留意见与原成果")
        XCTAssertEqual(original.request.command.messageContent?.text, text, "找回意见必须保留原始空白")
        var result: [String: Any] = ["node_id": 7, "run_id": "run-a", "session_id": "session-a", "turn_id": "new", "feedback": ["key": key, "fingerprint": fingerprint]]
        func validate(_ result: [String: Any]) throws {
            try decode(["device_id": "phone", "operation_id": "feedback", "workspace_id": "project", "route_id": "route", "state": "succeeded", "result": result], as: MobileCommandReceipt.self).validate(for: original)
        }
        try validate(result)
        let succeeded = result
        result["turn_id"] = "old"
        XCTAssertThrowsError(try validate(result), "旧成果不能冒充新修改任务")
        result["turn_id"] = "new"; result["session_id"] = "other"
        XCTAssertThrowsError(try validate(result), "不能接受其他会话回执")
        result["session_id"] = "session-a"; result["feedback"] = ["key": key, "fingerprint": String(repeating: "b", count: 64)]
        XCTAssertThrowsError(try validate(result), "新任务必须绑定所见成果")
        result.removeValue(forKey: "feedback")
        XCTAssertThrowsError(try validate(result), "普通消息成功不足以证明反馈关联")
        card["run_id"] = "reopened"; command["expected"] = card
        XCTAssertThrowsError(try pending(command).validate(), "重开的运行不可成为原意见目标")
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = ChatDraftStore(url: directory.appendingPathComponent("drafts.json"))
        let book = ChatDraftBook(store: store); await book.load()
        let ordinary = ChatDraft.Scope(machineID: original.machineID, workspaceID: "project", routeID: "route", nodeID: 7, runID: "run-a", sessionID: "session-a")
        let feedbackScope = ChatDraft.Scope(machineID: original.machineID, workspaceID: "project", routeID: "route", nodeID: 7,
                                           runID: "run-a", sessionID: "session-a", feedbackFingerprint: fingerprint)
        book.update(text, for: ordinary); book.update(text, for: feedbackScope); try await book.flush()
        let restored = ChatDraftBook(store: store); await restored.load()
        var withDraft = original
        withDraft.draftID = restored.draft(for: feedbackScope)?.id
        let receipt = try decode(["device_id": "phone", "operation_id": "feedback", "workspace_id": "project", "route_id": "route", "state": "succeeded", "result": succeeded], as: MobileCommandReceipt.self)
        try await restored.confirm(withDraft, receipt: receipt)
        XCTAssertNil(restored.draft(for: feedbackScope), "送达后只清除对应成果的意见草稿")
        XCTAssertEqual(restored.draft(for: ordinary)?.text, text, "同会话同文字的普通聊天草稿必须保留")
    }
    /// 交付正文必须与选中的准确回合一致；摘要只允许同项目同卡片的有序分页。
    ///
    /// 数据构造：node7/run-a/session-a/turn-a，序号2，指纹64个a，空正文；非法摘要改为node8；验收时间110大于保存时间100。
    /// 执行过程：读取摘要页 → 精确验收/错指纹/错项目/失败回合验收 → 读取固定成果 → 缺少回合证明/错误回合/另一卡摘要。
    /// 预期结果：只有准确成果和同项目验收通过；旧服务缺字段不能显示已验收；错卡与混合分页拒绝。
    func testFrozenDeliveryIdentityAndHistoryScope() throws {
        let key: [String: Any] = ["node_id": 7, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a"]
        func decode<T: Decodable>(_ value: Any, as: T.Type) throws -> T {
            try JSONDecoder().decode(T.self, from: JSONSerialization.data(withJSONObject: value))
        }
        let task = try decode(key, as: MobileTurnOutcome.Key.self)
        let scope = ProjectReviewRequest(workspaceID: "project", routeID: "route")
        let outcome: [String: Any] = ["key": key, "state": "completed"]
        let reference: [String: Any] = ["outcome": outcome, "fingerprint": String(repeating: "a", count: 64), "saved_at_unix": 100]
        var item = reference; item["sequence"] = 2; item["workspace_id"] = "project"; item["status"] = "available"; item["file_count"] = 0
        var page: [String: Any] = ["request": ["workspace_id": "project", "route_id": "route"], "node_id": 7, "items": [item]]
        let validPage = try decode(page, as: TaskDeliveryPage.self)
        try validPage.validate(for: scope, nodeID: 7, before: nil)
        XCTAssertEqual(validPage.items[0].outcome.key.turnID, "turn-a", "摘要保留准确回合")
        XCTAssertNil(validPage.items[0].acceptance, "旧服务没有验收字段时不能显示已验收")
        var acceptance: [String: Any] = ["workspace_id": "project", "route_id": "previous-route", "key": key,
            "fingerprint": String(repeating: "a", count: 64), "device_id": "phone", "operation_id": "accept", "accepted_at_unix": 110]
        var acceptedItem = item; acceptedItem["acceptance"] = acceptance
        func acceptedSummary(_ item: [String: Any]) throws -> TaskDeliverySummary {
            let value = try decode(item, as: TaskDeliverySummary.self)
            try value.validate(workspaceID: "project", nodeID: 7)
            return value
        }
        XCTAssertEqual(try acceptedSummary(acceptedItem).acceptance?.operationID, "accept", "历史标记来自原始验收，旧窗口路由不能抹去决定")
        acceptance["fingerprint"] = String(repeating: "b", count: 64); acceptedItem["acceptance"] = acceptance
        XCTAssertThrowsError(try acceptedSummary(acceptedItem), "错误成果指纹不能显示已验收")
        acceptance["fingerprint"] = String(repeating: "a", count: 64); acceptance["workspace_id"] = "other"; acceptedItem["acceptance"] = acceptance
        XCTAssertThrowsError(try acceptedSummary(acceptedItem), "相同回合的另一项目不能提供验收标记")
        acceptance["workspace_id"] = "project"; acceptedItem["acceptance"] = acceptance
        acceptedItem["outcome"] = ["key": key, "state": "failed"]
        XCTAssertThrowsError(try acceptedSummary(acceptedItem), "失败成果不能显示已验收")
        acceptedItem["outcome"] = outcome; acceptedItem["status"] = "baseline_missing"
        XCTAssertThrowsError(try acceptedSummary(acceptedItem), "不可用成果不能显示已验收")
        page["node_id"] = 8
        XCTAssertThrowsError(try decode(page, as: TaskDeliveryPage.self).validate(for: scope, nodeID: 7, before: nil), "另一卡片不能显示在当前成果页")
        page["node_id"] = 7; page["items"] = [item, item]
        XCTAssertThrowsError(try decode(page, as: TaskDeliveryPage.self).validate(for: scope, nodeID: 7, before: nil), "重复游标或回合必须拒绝")
        let selected = ProjectReviewRequest(workspaceID: "project", routeID: "route", deliveryKey: task)
        var response: [String: Any] = ["request": ["workspace_id": "project", "route_id": "route", "delivery_key": key], "review": ["files": [], "conflicted": false], "delivery": reference]
        let valid = try decode(response, as: ProjectReviewResponse.self)
        try valid.validate(for: selected)
        XCTAssertEqual(valid.delivery?.fingerprint, String(repeating: "a", count: 64), "正式结果必须带冻结指纹")
        response.removeValue(forKey: "delivery")
        XCTAssertThrowsError(try decode(response, as: ProjectReviewResponse.self).validate(for: selected), "当前目录预览没有交付证明不能显示成原任务成果")
        var wrong = reference; var other = key; other["turn_id"] = "turn-b"; wrong["outcome"] = ["key": other, "state": "completed"]
        response["delivery"] = wrong
        XCTAssertThrowsError(try decode(response, as: ProjectReviewResponse.self).validate(for: selected), "同run同session的另一回合也必须拒绝")
    }
    /// 验收命令可持久化回查，成功回执必须绑定完整回合和所见指纹。
    ///
    /// 数据构造：node7/run-a/session-a/turn-a；同项目历史成果；指纹为64个a，保存时间100。
    /// 执行过程：解码并验证命令 → 编码往返 → 核验成功 → 改turn或fingerprint → 删除时间证明。
    /// 预期结果：原请求和精确回执通过；另一回合、另一份内容及没有落盘时间均拒绝。
    func testAcceptanceCommandAndReceiptCannotMoveToAnotherDelivery() throws {
        func decode<T: Decodable>(_ value: Any, as: T.Type) throws -> T {
            try JSONDecoder().decode(T.self, from: JSONSerialization.data(withJSONObject: value))
        }
        let key: [String: Any] = ["node_id": 7, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a"]
        let fingerprint = String(repeating: "a", count: 64)
        let pending = try decode(["machineID": "mac-" + fingerprint, "deviceID": "phone", "request": [
            "operation_id": "accept", "workspace_id": "project", "route_id": "route", "grant_revision": 1,
            "command": ["type": "accept_delivery", "key": key, "fingerprint": fingerprint]]], as: PendingMobileCommand.self)
        try pending.validate()
        XCTAssertEqual(try JSONDecoder().decode(PendingMobileCommand.self, from: JSONEncoder().encode(pending)), pending, "重启回查必须保留完整决定，不转为另一次操作")
        var result: [String: Any] = ["node_id": 7, "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a", "fingerprint": fingerprint, "accepted_at_unix": 100]
        func receipt(_ result: [String: Any]) throws -> MobileCommandReceipt {
            try decode(["device_id": "phone", "operation_id": "accept", "workspace_id": "project", "route_id": "route", "state": "succeeded", "result": result], as: MobileCommandReceipt.self)
        }
        try receipt(result).validate(for: pending)
        result["turn_id"] = "next-turn"
        XCTAssertThrowsError(try receipt(result).validate(for: pending), "同一会话后续任务不能继承验收")
        result["turn_id"] = "turn-a"; result["fingerprint"] = String(repeating: "b", count: 64)
        XCTAssertThrowsError(try receipt(result).validate(for: pending), "相同任务的错误内容不能验收")
        result["fingerprint"] = fingerprint; result.removeValue(forKey: "accepted_at_unix")
        XCTAssertThrowsError(try receipt(result).validate(for: pending), "没有落盘时间不能显示验收成功")
    }

}

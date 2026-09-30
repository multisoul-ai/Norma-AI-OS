import XCTest
@testable import NormaCore

@MainActor final class ThreadTerminalControlTests: XCTestCase {
    /// 人工输入先落手机任务库，未决操作不能覆盖；成功回执只清理同版本草稿。
    /// 数据构造：1个原任务runtime-a:7、节点2、24行80列；2份同文不同revision的draft（5字节）。
    /// 执行过程：接管落盘/确认 → 保存输入 → 重开 → 尝试覆盖 → 编辑新草稿 → 旧成功/迟到accepted → 新输入成功。
    /// 预期结果：稳定操作和输入不丢、不进入自动派工；旧成功保留新草稿，匹配成功才清理。
    func testPersistedInputRequiresReceiptAndOnlyClearsItsOwnDraft() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspace.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        let original = try await seed(store)
        let read = try page(original, controlled: false)
        let claim = ThreadTerminalAction()
        let savedClaim = try await store.prepareThreadTerminalAction(operationID: original.id, action: claim, terminal: read)
        XCTAssertEqual(savedClaim.terminalOperation?.action.operationID, claim.operationID, "网络发送前必须保存稳定接管操作")
        try await store.applyThreadTerminalReceipt(controlReceipt(savedClaim, state: "accepted"), operationID: original.id)
        do {
            try await store.acknowledgeThreadTerminalOutcome(operationID: original.id, controlOperationID: claim.operationID)
            XCTFail("Mac尚在处理的accepted操作不能通过人工核对绕过")
        } catch { XCTAssertEqual(error as? WorkThreadError, .conflict, "仍在执行必须保持未决") }
        try await store.applyThreadTerminalReceipt(controlReceipt(savedClaim, state: "succeeded"), operationID: original.id)
        let controlled = try page(original, controlled: true)
        let draft = WorkThreadDraft(text: "draft")
        try await store.saveThreadTerminalDraft(draft, operationID: original.id)
        let action = try ThreadTerminalAction(claimOperationID: claim.operationID, page: controlled.page, input: .text(draft.text))
        let pending = try await store.prepareThreadTerminalAction(operationID: original.id, action: action,
            terminal: controlled, clearingDraftRevision: draft.revision)
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let restored = try await restarted.threadTask(operationID: original.id)
        XCTAssertEqual(restored.terminalOperation, pending.terminalOperation, "重开必须恢复同一人工操作和原回合")
        XCTAssertEqual(restored.terminalDraft, draft, "待确认输入不能提前清草稿")
        XCTAssertNil(restored.terminalOperation?.receipt, "本机保存不能伪造Mac成功")
        let automatic = try await restarted.pendingThreadTasks()
        XCTAssertTrue(automatic.isEmpty, "人工操作不能被普通自动派工队列重放")
        do {
            _ = try await restarted.prepareThreadTerminalAction(operationID: original.id,
                action: .release(claimOperationID: claim.operationID), terminal: controlled)
            XCTFail("未决输入不能被交还操作覆盖")
        } catch { XCTAssertTrue(error is WorkThreadError, "未决冲突应明确拒绝") }
        let newer = WorkThreadDraft(text: "draft")
        try await restarted.saveThreadTerminalDraft(newer, operationID: original.id)
        let success = try controlReceipt(pending, state: "succeeded")
        try await restarted.applyThreadTerminalReceipt(success, operationID: original.id)
        try await restarted.applyThreadTerminalReceipt(controlReceipt(pending, state: "accepted"), operationID: original.id)
        let late = try await restarted.threadTask(operationID: original.id)
        XCTAssertEqual(late.terminalOperation?.receipt, success, "迟到accepted不能回滚终态")
        XCTAssertEqual(late.terminalDraft, newer, "同文的新revision不能被旧成功清空")
        let next = try ThreadTerminalAction(claimOperationID: claim.operationID, page: controlled.page, input: .text(newer.text))
        let nextPending = try await restarted.prepareThreadTerminalAction(operationID: original.id, action: next,
            terminal: controlled, clearingDraftRevision: newer.revision)
        try await restarted.applyThreadTerminalReceipt(controlReceipt(nextPending, state: "succeeded"), operationID: original.id)
        let final = try await CollaborationWorkspaceStore(databaseURL: path).threadTask(operationID: original.id)
        XCTAssertNil(final.terminalDraft, "只有本次对应输入成功才能清除当前草稿")
        XCTAssertEqual(final.terminalOperation?.action.operationID, next.operationID, "最终必须保留可核对的原输入ID")
    }

    /// 受控后续回合仍绑定原任务；拒绝、结果未知、跨任务成功不能清理草稿或覆写待核对输入。
    /// 数据构造：原turn-a，人工续入turn-b；1份draft=5字节，错误task_ref使用另一runtime但同数字7。
    /// 执行过程：普通读拒绝后续回合 → 受控页准备输入 → 篡改成功回执 → 拒绝 → 新操作结果未知。
    /// 预期结果：普通读取规则不放宽；篡改响应无写入；拒绝/未知均保留草稿且未知不能被新操作覆盖。
    func testControlledTurnAndUncertainReceiptKeepOriginalIdentityAndDraft() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("store.sqlite"))
        let original = try await seed(store)
        let controlled = try page(original, controlled: true, turn: "turn-b")
        let receipt = try XCTUnwrap(original.receipt)
        XCTAssertThrowsError(try controlled.validate(for: controlled.request, task: original.task, receipt: receipt),
                             "普通读取不得接受人工后续回合或输入权")
        let draft = WorkThreadDraft(text: "draft")
        try await store.saveThreadTerminalDraft(draft, operationID: original.id)
        let claim = UUID().uuidString
        let action = try ThreadTerminalAction(claimOperationID: claim, page: controlled.page, input: .text(draft.text))
        let pending = try await store.prepareThreadTerminalAction(operationID: original.id, action: action, terminal: controlled,
                                                                  clearingDraftRevision: draft.revision)
        XCTAssertEqual(pending.terminalOperation?.key.turnID, "turn-a", "人工新回合不能改写原任务来源")
        XCTAssertEqual(pending.terminalOperation?.action.expected?.turnID, "turn-b", "输入应核对当前真实屏幕回合")
        var wrongBody = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(controlReceipt(pending, state: "succeeded"))) as? [String: Any])
        var result = try XCTUnwrap(wrongBody["result"] as? [String: Any]); result["task_ref"] = "other-runtime:7"; wrongBody["result"] = result
        let wrong = try JSONDecoder().decode(MobileCommandReceipt.self, from: JSONSerialization.data(withJSONObject: wrongBody))
        do { try await store.applyThreadTerminalReceipt(wrong, operationID: original.id); XCTFail("其他运行同数字任务不能作为成功") }
        catch { XCTAssertEqual(error as? MobileError, .invalidResponse, "必须准确拒绝跨任务回执") }
        let unchanged = try await store.threadTask(operationID: original.id)
        XCTAssertNil(unchanged.terminalOperation?.receipt, "非法成功不能写入任务库")
        try await store.applyThreadTerminalReceipt(controlReceipt(pending, state: "rejected"), operationID: original.id)
        let rejected = try await store.threadTask(operationID: original.id)
        XCTAssertEqual(rejected.terminalDraft, draft, "拒绝不能丢失输入正文")
        let next = try ThreadTerminalAction(claimOperationID: claim, page: controlled.page, input: .text(draft.text))
        let nextPending = try await store.prepareThreadTerminalAction(operationID: original.id, action: next, terminal: controlled,
                                                                      clearingDraftRevision: draft.revision)
        try await store.applyThreadTerminalReceipt(controlReceipt(nextPending, state: "outcome_unknown"), operationID: original.id)
        do {
            _ = try await store.prepareThreadTerminalAction(operationID: original.id, action: .release(claimOperationID: claim), terminal: controlled)
            XCTFail("未知输入结果不能自动放弃或覆盖")
        } catch { XCTAssertTrue(error is WorkThreadError, "需要用户核对的输入保持未解决") }
        let unknown = try await store.threadTask(operationID: original.id)
        XCTAssertEqual(unknown.terminalDraft, draft, "结果不明不能清理草稿")
        XCTAssertEqual(unknown.terminalOperation?.receipt?.state, "outcome_unknown", "未知状态必须持久保留")
        do {
            try await store.acknowledgeThreadTerminalOutcome(operationID: original.id, controlOperationID: action.operationID)
            XCTFail("旧输入的核对不能解锁新输入")
        } catch { XCTAssertEqual(error as? WorkThreadError, .conflict, "必须核对当前稳定操作ID") }
        try await store.acknowledgeThreadTerminalOutcome(operationID: original.id, controlOperationID: next.operationID)
        let acknowledged = try await store.threadTask(operationID: original.id)
        XCTAssertFalse(acknowledged.terminalOperation?.unresolved ?? true, "用户明确核对后才允许下一次操作")
        XCTAssertEqual(acknowledged.terminalOperation?.receipt?.state, "outcome_unknown", "人工核对不能伪造成功回执")
        XCTAssertEqual(acknowledged.terminalDraft, draft, "核对结果不能清理输入草稿")
    }

    private let mac = "mac-" + String(repeating: "a", count: 64)
    func seed(_ store: CollaborationWorkspaceStore) async throws -> ThreadTaskOutboxOperation {
        var workspace = try await store.create(name: "终端")
        workspace.projects = [.init(machineID: mac, projectID: "project-a", name: "iOS", machineName: "Mac")]
        let agent = WorkspaceAgentReference(machineID: mac, projectID: "project-a", definitionID: 2, name: "开发", profile: "Codex")
        workspace.agents = [agent]; try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "实现")
        let task = try await store.appendTaskMessage(threadID: thread.id, messageID: UUID().uuidString, text: "@开发 实现",
            operationID: UUID().uuidString, target: agent, task: "实现", contextVersion: 1, context: "")
        let outbox = try await store.prepareThreadDelivery(messageID: task.task.sourceMessageID)
        let payload = try XCTUnwrap(outbox.message)
        // 第1条fixture接收事件；固定unix值用于可复现数据，并非真实Mac签收。
        try await store.applyThreadReceipt(.init(sourceMacID: mac, collaborationWorkspaceID: workspace.id, threadID: thread.id,
            operationID: outbox.id, messageID: payload.messageID, originDeviceID: payload.originDeviceID,
            fingerprint: payload.fingerprint(machineID: mac), receivedAtUnix: 1_790_000_000, eventID: mac + ":1", cursor: 1, state: "received"))
        let receipt = try ThreadTaskReceipt(binding: .init(sourceMacID: mac, collaborationWorkspaceID: workspace.id,
            threadID: thread.id, sourceOperationID: task.task.sourceOperationID, sourceMessageID: task.task.sourceMessageID,
            payload: String(decoding: JSONEncoder().encode(task.task), as: UTF8.self)), fingerprint: task.task.fingerprint(machineID: mac),
            receipt: .init(deviceID: "phone-a", operationID: task.id, workspaceID: "project-a", routeID: "route-a", state: "succeeded",
                result: .init(status: "accepted", message: nil, nodeID: 2, taskID: 7, taskRef: "runtime-a:7"), error: nil))
        try await store.applyThreadTaskReceipt(receipt, deviceID: "phone-a")
        return try await store.threadTask(operationID: task.id)
    }

    func page(_ operation: ThreadTaskOutboxOperation, controlled: Bool, turn: String = "turn-a") throws -> ThreadTaskTerminal {
        let request = try ThreadTerminalRequest(task: operation.task, receipt: XCTUnwrap(operation.receipt))
        let body: [String: Any] = ["request": try JSONSerialization.jsonObject(with: JSONEncoder().encode(request)),
            "scope": ["workspace_id": "project-a", "route_id": "route-a", "node_id": 2, "run_id": "run-a", "session_id": "session-a"],
            "key": key,
            "page": ["card": ["node_id": 2, "profile": "Codex", "title": "任务", "mobile_control": true, "state": "thinking",
                             "run_id": "run-a", "session_id": "session-a", "turn_id": turn],
                     "revision": 9, "screen_signature": 17, "input_sequence": 0, "start_line": 0, "total_lines": 24,
                     "cols": 80, "running": true, "input_ready": controlled, "rows": Array(repeating: "输出", count: 24)]]
        return try JSONDecoder().decode(ThreadTaskTerminal.self, from: JSONSerialization.data(withJSONObject: body))
    }
    var key: [String: Any] { ["node_id": 2, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a"] }
    func controlReceipt(_ operation: ThreadTaskOutboxOperation, state: String) throws -> MobileCommandReceipt {
        let action = try XCTUnwrap(operation.terminalOperation?.action)
        let status = action.type == .claim ? "controlled" : (action.type == .input ? "written" : "released")
        var body: [String: Any] = ["device_id": "phone-a", "operation_id": action.operationID,
            "workspace_id": "project-a", "route_id": "route-a", "state": state]
        if state == "succeeded" { body["result"] = ["status": status, "key": key, "task_ref": "runtime-a:7"] }
        if state == "rejected" { body["error"] = "terminal_changed" }
        if state == "outcome_unknown" { body["error"] = "outcome_unknown" }
        return try JSONDecoder().decode(MobileCommandReceipt.self, from: JSONSerialization.data(withJSONObject: body))
    }
}

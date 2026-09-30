import XCTest
import SQLite3
@testable import NormaCore

@MainActor final class ThreadFollowupStoreTests: XCTestCase {
    /// 原任务追问原子保存独立来源，重启只恢复一次回复且不改写原任务。
    /// 数据构造：1个原任务、2个线程、1条追问；同文草稿有两个revision；Provider回合a→b。
    /// 执行过程：原派工确认→失败注入回滚→保存追问→新草稿保留→来源/设备拒绝→签收→sent→迟到queued/伪造锚点→重启。
    /// 预期结果：新草稿保留；Agent回复只一条且在原线程；只有原任务，无新增派工；旧回执不倒退或重发。
    func testFollowupSurvivesRestartAndAppendsOnlyItsOwnReply() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "追问")
        let mac = "mac-" + String(repeating: "a", count: 64)
        let agent = WorkspaceAgentReference(machineID: mac, projectID: "project-a", definitionID: 2, name: "开发", profile: "Codex")
        workspace.projects = [.init(machineID: mac, projectID: "project-a", name: "iOS", machineName: "Mac")]
        workspace.agents = [agent]; try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let other = try await store.createThread(workspaceID: workspace.id, title: "另一件事")
        let task = try await store.appendTaskMessage(threadID: thread.id, messageID: UUID().uuidString,
            text: "改首屏", operationID: UUID().uuidString, target: agent, task: "改首屏", contextVersion: 1, context: "")
        try await signSource(store, id: task.task.sourceMessageID, mac: mac)
        let accepted = try ThreadTaskReceipt(binding: .init(sourceMacID: mac, collaborationWorkspaceID: workspace.id,
            threadID: thread.id, sourceOperationID: task.task.sourceOperationID, sourceMessageID: task.task.sourceMessageID,
            payload: String(decoding: JSONEncoder().encode(task.task), as: UTF8.self)), fingerprint: task.task.fingerprint(machineID: mac),
            receipt: .init(deviceID: "phone-a", operationID: task.id, workspaceID: "project-a", routeID: "route-a",
                state: "succeeded", result: .init(status: "accepted", message: nil, nodeID: 2, taskID: 7, taskRef: "runtime-a:7"), error: nil))
        try await store.applyThreadTaskReceipt(accepted, deviceID: "phone-a")
        // 原派工只有project-a；追问时新增project-b，验证追问自己的范围也须保留。
        workspace.projects.append(.init(machineID: mac, projectID: "project-b", name: "API", machineName: "Mac"))
        try await store.save(workspace)
        let draft = WorkThreadDraft(text: "为什么这样修改？")
        let newer = WorkThreadDraft(text: draft.text)
        try await store.saveMessageDraft(newer, threadID: thread.id)
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(path.path, &db), SQLITE_OK, "打开隔离数据库")
        defer { sqlite3_close(db) }
        XCTAssertEqual(sqlite3_exec(db, "CREATE TRIGGER fail_followup BEFORE INSERT ON thread_followup_outbox BEGIN SELECT RAISE(ABORT,'fixture'); END;", nil, nil, nil),
            SQLITE_OK, "注入追问事务失败")
        do {
            _ = try await store.appendThreadFollowup(parentOperationID: task.id, messageID: UUID().uuidString,
                text: newer.text, operationID: UUID().uuidString, clearingDraftRevision: newer.revision)
            XCTFail("追问落盘失败不能留下半条来源消息")
        } catch { XCTAssertTrue(error is WorkThreadError, "应报告持久化错误") }
        let rolledBackMessages = try await store.messages(threadID: thread.id)
        let rolledBackDrafts = try await store.messageDrafts()
        XCTAssertEqual(rolledBackMessages.map(\.text), ["改首屏"], "来源和追问必须同事务回滚")
        XCTAssertEqual(rolledBackDrafts[thread.id], newer, "失败不能清掉用户输入")
        XCTAssertEqual(sqlite3_exec(db, "DROP TRIGGER fail_followup", nil, nil, nil), SQLITE_OK, "恢复正常存储")
        let operation = try await store.appendThreadFollowup(parentOperationID: task.id, messageID: UUID().uuidString,
            text: draft.text, operationID: UUID().uuidString, clearingDraftRevision: draft.revision)
        let drafts = try await store.messageDrafts()
        XCTAssertEqual(drafts[thread.id], newer, "发送不能清掉同文但新revision的草稿")
        do { _ = try await store.prepareThreadFollowup(operationID: operation.id, deviceID: "phone-a"); XCTFail("来源未送达不能向Agent发送追问") }
        catch { XCTAssertTrue(error is ThreadTaskError, "应报告来源尚未送达而非悄悄入队") }
        try await signSource(store, id: operation.followup.sourceMessageID, mac: mac)
        do { _ = try await store.prepareThreadFollowup(operationID: operation.id, deviceID: "other-phone"); XCTFail("其他配对设备不能继续原任务") }
        catch { XCTAssertTrue(error is MobileError, "设备不符应报告配对身份错误") }
        var reduced = workspace; reduced.projects.removeLast()
        try await store.save(reduced)
        do { _ = try await store.prepareThreadFollowup(operationID: operation.id, deviceID: "phone-a"); XCTFail("移除追问新增范围后不能沿用父任务检查放行") }
        catch { XCTAssertTrue(error is ThreadDeliveryError, "应说明追问范围已经变化") }
        try await store.save(workspace)
        _ = try await store.prepareThreadFollowup(operationID: operation.id, deviceID: "phone-a")
        let queued = try receipt(operation, sent: false)
        let finished = try receipt(operation, sent: true)
        try await store.applyThreadFollowupReceipt(queued)
        try await store.applyThreadFollowupReceipt(finished)
        try await store.applyThreadFollowupReceipt(finished)
        try await store.applyThreadFollowupReceipt(queued)
        var changed = try JSONSerialization.jsonObject(with: JSONEncoder().encode(finished)) as! [String: Any]
        var key = changed["original_key"] as! [String: Any]
        key["turn_id"] = "another-original"; changed["original_key"] = key
        let forged = try JSONDecoder().decode(ThreadFollowupReceipt.self, from: JSONSerialization.data(withJSONObject: changed))
        do { try await store.applyThreadFollowupReceipt(forged); XCTFail("不能替换首次Mac签收的原回合锚点") }
        catch { XCTAssertTrue(error is ThreadTaskError, "应拒绝身份冲突回执") }
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let messages = try await restarted.messages(threadID: thread.id)
        let unrelated = try await restarted.messages(threadID: other.id)
        let saved = try await restarted.threadFollowup(operationID: operation.id)
        let tasks = try await restarted.threadTasks(threadID: thread.id)
        let pending = try await restarted.pendingThreadFollowups()
        XCTAssertEqual(messages.map(\.text), ["改首屏", draft.text, "因为这样保留目标。"], "重连/重复/迟到回执只产生一次新回复")
        XCTAssertEqual(messages.last?.replyToMessageID, operation.followup.sourceMessageID, "回复指向本次来源")
        XCTAssertEqual(messages.last?.taskOperationID, task.id, "仍属于原任务")
        XCTAssertTrue(unrelated.isEmpty, "不能把追问或回复写入其他线程")
        XCTAssertEqual(tasks.count, 1, "追问不能变成第二个派工任务")
        XCTAssertEqual(tasks.first?.receipt, accepted, "原任务回执不得被新回合替代")
        XCTAssertEqual(saved.receipt, finished, "迟到queued不能覆盖已确认的新回合")
        XCTAssertTrue(pending.isEmpty, "已完成的追问不进入自动重发队列")
    }

    /// 回复对象在空正文时也能恢复，且不能把一个线程的任务绑到另一个线程。
    /// 数据构造：同一 Workspace 的两个线程；首线程有一个待发任务，空正文含其原任务 ID。
    /// 执行过程：解码带回复对象的草稿→保存→重建 Store→尝试写入另一线程与成果/终端草稿。
    /// 预期结果：空正文及对象原样保存；其他线程、成果退回和终端输入不得接收该对象，原草稿保留。
    func testFollowupDraftPreservesTargetAndRejectsOtherScopes() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        let mac = "mac-" + String(repeating: "a", count: 64)
        let agent = WorkspaceAgentReference(machineID: mac, projectID: "ios", definitionID: 2, name: "开发", profile: "Codex")
        var workspace = try await store.create(name: "草稿范围")
        workspace.projects = [.init(machineID: mac, projectID: "ios", name: "iOS", machineName: "Mac")]
        workspace.agents = [agent]; try await store.save(workspace)
        let first = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let second = try await store.createThread(workspaceID: workspace.id, title: "另一个线程")
        let task = try await store.appendTaskMessage(threadID: first.id, messageID: UUID().uuidString,
            text: "改首屏", operationID: UUID().uuidString, target: agent, task: "改首屏", contextVersion: 1, context: "")
        let payload: [String: Any] = ["text": "", "revision": UUID().uuidString, "followupTaskID": task.id]
        let draft = try JSONDecoder().decode(WorkThreadDraft.self, from: JSONSerialization.data(withJSONObject: payload))
        try await store.saveMessageDraft(draft, threadID: first.id)
        let reopened = CollaborationWorkspaceStore(databaseURL: path)
        let saved = try await reopened.messageDrafts()
        let encoded = try JSONSerialization.jsonObject(with: JSONEncoder().encode(XCTUnwrap(saved[first.id]))) as! [String: Any]
        XCTAssertEqual(encoded["followupTaskID"] as? String, task.id, "空正文必须保留明确的追问对象")
        XCTAssertEqual(saved[first.id]?.text, "", "选择对象不能偷偷插入提及或发送指令")
        do { try await reopened.saveMessageDraft(draft, threadID: second.id); XCTFail("另一线程不能引用首线程的任务") }
        catch { XCTAssertTrue(error is WorkThreadError, "跨线程草稿必须被拒绝") }
        do { try await reopened.saveThreadReviewDraft(draft, operationID: task.id); XCTFail("成果退回草稿不能夹带追问对象") }
        catch { XCTAssertTrue(error is WorkThreadError, "不支持的草稿目标必须被拒绝") }
        do { try await reopened.saveThreadTerminalDraft(draft, operationID: task.id); XCTFail("终端输入草稿不能夹带追问对象") }
        catch { XCTAssertTrue(error is WorkThreadError, "不支持的草稿目标必须被拒绝") }
        let final = try await reopened.messageDrafts()
        XCTAssertNil(final[second.id], "失败不能留下跨线程草稿")
        XCTAssertEqual(final[first.id], draft, "失败不能破坏原线程草稿")
    }

    private func signSource(_ store: CollaborationWorkspaceStore, id: String, mac: String) async throws {
        let outbox = try await store.prepareThreadDelivery(messageID: id)
        let source = try XCTUnwrap(outbox.message)
        //1为测试的首个事件游标，时间戳为固定有效秒数；这是存储fixture，不冒充网络签收。
        try await store.applyThreadReceipt(.init(sourceMacID: mac, collaborationWorkspaceID: source.collaborationWorkspaceID,
            threadID: source.threadID, operationID: source.operationID, messageID: id, originDeviceID: source.originDeviceID,
            fingerprint: source.fingerprint(machineID: mac), receivedAtUnix: 1_790_000_000, eventID: mac + ":1", cursor: 1, state: "received"))
    }

    private func receipt(_ operation: ThreadFollowupOutboxOperation, sent: Bool) throws -> ThreadFollowupReceipt {
        let followup = operation.followup
        let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]
        let key: [String: Any] = ["node_id": 2, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a"]
        var newKey = key; newKey["turn_id"] = "turn-b"
        var queue: [String: Any] = ["device_id": "phone-a", "operation_id": operation.id, "workspace_id": "project-a", "route_id": "route-a",
            "node_id": 2, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "text": followup.text, "state": sent ? "sent" : "queued"]
        if sent { queue["result"] = ["node_id": 2, "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-b"] }
        let outcome: [String: Any] = ["key": newKey, "state": "completed", "summary": "因为这样保留目标。"]
        var response: [String: Any] = ["original_key": key, "binding": ["source_mac_id": operation.machineID,
            "collaboration_workspace_id": followup.collaborationWorkspaceID, "thread_id": followup.threadID,
            "source_operation_id": followup.sourceOperationID, "source_message_id": followup.sourceMessageID,
            "payload": String(decoding: try encoder.encode(followup), as: UTF8.self)],
            "fingerprint": try followup.fingerprint(machineID: operation.machineID), "queued_message": queue,
            "receipt": ["device_id": "phone-a", "operation_id": operation.id, "workspace_id": "project-a", "route_id": "route-a", "state": "succeeded",
                "result": ["node_id": 2, "run_id": "run-a", "session_id": "session-a", "queue_id": operation.id, "status": "queued"]]]
        if sent {
            response["outcome"] = outcome
            //1份新成果；明确缺少baseline，file_count必须为0，不能伪装已有文件。
            response["delivery"] = ["sequence": 1, "workspace_id": "project-a", "outcome": outcome, "fingerprint": String(repeating: "b", count: 64),
                "saved_at_unix": 1_790_000_000, "status": "baseline_missing", "file_count": 0]
        }
        return try JSONDecoder().decode(ThreadFollowupReceipt.self, from: JSONSerialization.data(withJSONObject: response))
    }
}

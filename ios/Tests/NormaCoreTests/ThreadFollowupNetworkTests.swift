import XCTest
import SQLite3
@testable import NormaCore

@MainActor final class ThreadFollowupNetworkTests: XCTestCase {
    /// Swift从正式中继恢复原追问，重复提交和重连不得新建执行。
    /// 数据构造：一条已由Rust队列确认的追问，原回合original-turn、新回合followup-turn。
    /// 执行过程：读取父任务 → 重试原追问 → 断线重连 → 以真实来源签收重建手机fixture → 统一循环恢复 → 重复恢复。
    /// 预期结果：新回复及成果归属准确；原route不变；模拟Mac丢账本时拒绝重发，Rust另外断言没有额外命令。
    /// 层级：真实Swift/Node/Rust网络与SQLite，Provider前台是明确fixture，不是真实Agent/UI。
    func testTypedFollowupRecoversOriginalReceiptWithoutResending() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_NATIVE_FOLLOWUP_FIXTURE"] else {
            throw XCTSkip("由正式本机中继追问专项提供隔离fixture")
        }
        struct Fixture: Decodable {
            let port: UInt16
            let machineID: String
            let task: ThreadTaskOperation
            let followup: ThreadFollowupOperation
            let parentSource: ThreadMessageOperation
            let followupSource: ThreadMessageOperation
            let cloud: CloudDeviceCredential
            let mac: DeviceCredential
        }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let client = try CollaborationRelayClient(localPort: fixture.port)
        let connection = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        let capabilities = await connection.capabilities
        XCTAssertTrue(capabilities.contains("thread_task_followup_v1"), "必须显式协商原任务追问能力")
        let saved = try await connection.threadTaskReceipt(for: fixture.task)
        let original = try XCTUnwrap(saved)
        let key = try XCTUnwrap(original.execution?.execution.delivery?.outcome.key)
        var parent = ThreadTaskOutboxOperation(machineID: fixture.machineID, task: fixture.task)
        parent.deviceID = fixture.mac.deviceID; parent.receipt = original
        let received = try await connection.threadFollowupReceipt(for: fixture.followup, parent: parent, originalKey: key)
        let receipt = try XCTUnwrap(received)
        XCTAssertEqual(receipt.queuedMessage.state, "sent", "Provider确认后才能标记已发给Agent")
        XCTAssertEqual(receipt.outcome?.summary, "追问回复：首屏需要保留当前目标。", "真实持久化的新回合回复必须完整返回")
        XCTAssertEqual(receipt.delivery?.outcome.key.turnID, "followup-turn", "追问成果属于新回合")
        XCTAssertNotEqual(receipt.outcome?.key.turnID, key.turnID, "不能把原任务完成态当作追问回复")
        let retry = try await connection.submitThreadFollowup(fixture.followup, parent: parent, originalKey: key,
            routeID: "unused-retry-route", grantRevision: 1)
        XCTAssertEqual(retry, receipt, "同操作重试必须返回原回执，不重发或换route")
        await connection.close()
        let recovered = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        let replay = try await recovered.threadFollowupReceipt(for: fixture.followup, parent: parent, originalKey: key)
        XCTAssertEqual(replay, receipt, "断线恢复只读取同一操作结果")
        let parentSourceReceipt = try await recovered.threadReceipt(for: fixture.parentSource)
        let sourceReceipt = try await recovered.threadReceipt(for: fixture.followupSource)
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: database)
        try await store.importFollowupFixture(parent: parent, sources: [fixture.parentSource, fixture.followupSource])
        try await store.applyThreadReceipt(XCTUnwrap(parentSourceReceipt))
        try await store.applyThreadTaskReceipt(original, deviceID: fixture.mac.deviceID)
        try await store.applyThreadReceipt(XCTUnwrap(sourceReceipt))
        let pending = try await store.appendThreadFollowup(parentOperationID: parent.id, messageID: fixture.followup.sourceMessageID,
            text: fixture.followup.text, operationID: fixture.followup.operationID)
        XCTAssertNil(pending.receipt, "模拟手机丢失回执，不能预先标记完成")
        let restarted = CollaborationWorkspaceStore(databaseURL: database)
        let attempt = try await restarted.deliverPendingThreadTasks(using: recovered)
        XCTAssertEqual(attempt?.operationID, pending.id, "已完成的父任务不能掩盖独立待恢复追问")
        XCTAssertNil(attempt?.error, "正式网络应恢复同一追问")
        let completed = try await restarted.threadFollowup(operationID: pending.id)
        XCTAssertEqual(completed.receipt, receipt, "必须落盘Mac原回执而不是生成新发送")
        _ = try await restarted.deliverThreadFollowup(operationID: pending.id, using: recovered)
        let messages = try await restarted.messages(threadID: pending.followup.threadID)
        XCTAssertEqual(messages.filter { $0.replyToMessageID == pending.followup.sourceMessageID }.map(\.text),
            ["追问回复：首屏需要保留当前目标。"], "丢回执与重复恢复只能追加一份Agent回复")
        let after = try await restarted.pendingThreadFollowups()
        XCTAssertTrue(after.isEmpty, "已收到成果的追问不得继续占用投递循环")
        let tasks = try await restarted.threadTasks(threadID: pending.followup.threadID)
        XCTAssertEqual(tasks.map(\.id), [parent.id], "追问恢复不得创建第二项派工")
        // Model only ledger loss: this local remembered acknowledgement is deliberately absent on the Mac.
        let missing = try await restarted.appendThreadFollowup(parentOperationID: parent.id, messageID: UUID().uuidString,
            text: "保留丢失回执的追问", operationID: UUID().uuidString)
        _ = try await restarted.deliverThreadMessage(messageID: missing.followup.sourceMessageID, using: recovered)
        var json = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(receipt)) as? [String: Any])
        json["outcome"] = nil; json["delivery"] = nil
        var binding = try XCTUnwrap(json["binding"] as? [String: Any])
        binding["source_operation_id"] = missing.followup.sourceOperationID
        binding["source_message_id"] = missing.followup.sourceMessageID
        binding["payload"] = String(decoding: try JSONEncoder().encode(missing.followup), as: UTF8.self)
        json["binding"] = binding; json["fingerprint"] = try missing.followup.fingerprint(machineID: fixture.machineID)
        var command = try XCTUnwrap(json["receipt"] as? [String: Any])
        command["operation_id"] = missing.id
        var result = try XCTUnwrap(command["result"] as? [String: Any]); result["queue_id"] = missing.id
        command["result"] = result; json["receipt"] = command
        var queue = try XCTUnwrap(json["queued_message"] as? [String: Any])
        queue["operation_id"] = missing.id; queue["text"] = missing.followup.text
        queue["state"] = "queued"; queue["result"] = nil; json["queued_message"] = queue
        let remembered = try JSONDecoder().decode(ThreadFollowupReceipt.self, from: JSONSerialization.data(withJSONObject: json))
        try await restarted.applyThreadFollowupReceipt(remembered)
        do { _ = try await restarted.deliverThreadFollowup(operationID: missing.id, using: recovered); XCTFail("Mac丢账本不能自动重发已接收的追问") }
        catch ThreadTaskError.receiptMissing { }
        let preserved = try await restarted.threadFollowup(operationID: missing.id)
        XCTAssertEqual(preserved.receipt, remembered, "查询为空必须保留本机签收证据")
        let absent = try await recovered.threadFollowupReceipt(for: missing.followup, parent: parent)
        XCTAssertNil(absent, "恢复失败不能在Mac上创建第二次执行")
        await recovered.close()
    }
}

// Only seed identities already issued by the real Mac fixture. Receipts still use production validators.
private extension CollaborationWorkspaceStore {
    func importFollowupFixture(parent: ThreadTaskOutboxOperation, sources: [ThreadMessageOperation]) throws {
        try threadDatabase(writing: true) { db in
            let task = parent.task
            let origin = try prepare(db, "INSERT INTO collaboration_origin(slot,id) VALUES(1,?)")
            defer { sqlite3_finalize(origin) }
            try bind(origin, [sources[0].originDeviceID])
            guard sqlite3_step(origin) == SQLITE_DONE else { throw WorkThreadError.damagedStore }
            let workspace = CollaborationWorkspace(id: task.collaborationWorkspaceID, name: "网络恢复", projects: [
                .init(machineID: parent.machineID, projectID: task.projectID, name: "iOS", machineName: "Mac")],
                agents: [.init(machineID: parent.machineID, projectID: task.projectID, definitionID: task.agentDefinitionID, name: "开发", profile: "Codex")],
                archived: false, createdAt: .now, updatedAt: .now)
            try insertThreadRecord(db, "INSERT INTO collaboration_workspaces(id,updated_at,configuration) VALUES(?,?,?)",
                                   [workspace.id, "1790000000"], workspace)
            let thread = WorkThread(id: task.threadID, workspaceID: workspace.id, title: "原线程", sourceIdeaID: nil,
                sourceExcerpt: nil, originDeviceID: sources[0].originDeviceID, createdAt: .now)
            try insertThreadRecord(db, "INSERT INTO work_threads(id,workspace_id,payload) VALUES(?,?,?)", [thread.id, workspace.id], thread)
            for source in sources {
                let message = WorkThreadMessage(id: source.messageID, workspaceID: workspace.id, threadID: thread.id,
                    author: "user", originDeviceID: source.originDeviceID, text: source.text, createdAt: .now, state: .pending)
                try insertThreadRecord(db, "INSERT INTO work_messages(id,workspace_id,thread_id,payload) VALUES(?,?,?,?)",
                                       [message.id, workspace.id, thread.id], message)
                let outbox = ThreadOutboxOperation(id: source.operationID, messageID: message.id, machineID: parent.machineID, message: source)
                try insertThreadRecord(db, "INSERT INTO thread_outbox(id,message_id,payload) VALUES(?,?,?)", [outbox.id, message.id], outbox)
            }
            // The parent receipt is applied afterward so its original reply is inserted atomically too.
            let taskOutbox = ThreadTaskOutboxOperation(machineID: parent.machineID, task: task, target: workspace.agents[0])
            try writeTaskOutbox(db, taskOutbox)
        }
    }
}

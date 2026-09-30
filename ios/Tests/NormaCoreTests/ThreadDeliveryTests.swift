import XCTest
import SQLite3
@testable import NormaCore

@MainActor final class ThreadDeliveryTests: XCTestCase {
    /// 消息落盘即有稳定待发操作；目标准备与真实回执应用均可跨重启且不能串线程。
    /// 数据构造：1 个 Workspace、2 个线程、2 条消息、1 台 Mac/项目；没有网络时状态只能 pending。
    /// 执行过程：离线保存 → 重建 Store → 加项目 → 冻结操作 → 改设置 → 校验错回执 → 应用原回执 → 重开。
    /// 预期结果：原操作 ID/原文不变；目标不被改派；错误回执不改状态；另一线程仍待发。
    func testOutboxFreezesTargetAndAppliesOnlyMatchingReceipt() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: dir) }
        let path = dir.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "首屏")
        let a = try await store.createThread(workspaceID: workspace.id, title: "A")
        let b = try await store.createThread(workspaceID: workspace.id, title: "B")
        let first = try await store.appendMessage(threadID: a.id, id: UUID().uuidString, text: "先讨论首屏")
        let second = try await store.appendMessage(threadID: b.id, id: UUID().uuidString, text: "另一件事")
        let pending = try await store.pendingThreadDeliveries()
        XCTAssertEqual(pending.count, 2, "每条已发送本机消息都必须有原子持久化的待发操作")
        let operationID = try XCTUnwrap(pending.first { $0.messageID == first.id }?.id)
        do { _ = try await store.prepareThreadDelivery(messageID: first.id); XCTFail("无关联电脑时不能猜测目标") }
        catch { XCTAssertTrue(error is ThreadDeliveryError, "无目标需保留待发并明确报错") }
        let mac = "mac-" + String(repeating: "a", count: 64)
        workspace.projects = [.init(machineID: mac, projectID: "project-a", name: "iOS", machineName: "Mac")]
        try await store.save(workspace)
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let prepared = try await restarted.prepareThreadDelivery(messageID: first.id)
        let payload = try XCTUnwrap(prepared.message)
        XCTAssertEqual(prepared.id, operationID, "准备目标和重启都不能重新生成操作")
        XCTAssertEqual(payload.text, first.text, "冻结载荷必须保留原文")
        workspace.projects = [.init(machineID: "mac-" + String(repeating: "b", count: 64), projectID: "project-b", name: "API", machineName: "另一台 Mac")]
        try await store.save(workspace)
        do { _ = try await restarted.prepareThreadDelivery(messageID: first.id); XCTFail("配置变化不能把原操作改派到另一台 Mac") }
        catch { XCTAssertTrue(error is ThreadDeliveryError, "冻结目标已移除时应停止发送") }
        let receipt = ThreadMessageReceipt(sourceMacID: mac, collaborationWorkspaceID: workspace.id,
            threadID: a.id, operationID: operationID, messageID: first.id, originDeviceID: payload.originDeviceID,
            fingerprint: try payload.fingerprint(machineID: mac), receivedAtUnix: 1_790_000_000,
            eventID: mac + ":1", cursor: 1, state: "received")
        var wrong = receipt; wrong.threadID = b.id
        do { try await store.applyThreadReceipt(wrong); XCTFail("串线程回执必须拒绝") }
        catch { XCTAssertTrue(error is ThreadDeliveryError, "错回执不能更改本地消息") }
        let before = try await store.messages(threadID: a.id)
        XCTAssertEqual(before.first?.state, .pending, "仅转发或错误回执不能改为已送达")
        try await store.applyThreadReceipt(receipt)
        try await store.applyThreadReceipt(receipt)
        let reopened = CollaborationWorkspaceStore(databaseURL: path)
        let delivered = try await reopened.messages(threadID: a.id)
        let remaining = try await reopened.pendingThreadDeliveries()
        XCTAssertEqual(delivered.map(\.state), [.delivered], "Mac 真实回执必须持久改变原消息状态")
        XCTAssertEqual(remaining.map(\.messageID), [second.id], "另一线程仍待发，不得一并标成送达")
        XCTAssertFalse(remaining.contains { $0.id == operationID }, "重复回执不能重建原 outbox")
    }

    /// 待发操作写失败必须保留输入草稿，不能出现只有消息、没有待发操作的半次发送。
    /// 数据构造：1 条草稿；SQLite trigger 拒绝 outbox INSERT，预期新增消息数 0。
    /// 执行过程：建线程/保存草稿 → 注入写失败 → 发送 → 重开读取消息和草稿。
    /// 预期结果：发送报错，原消息未插入，草稿 revision 与正文完整保留。
    func testOutboxWriteFailurePreservesDraftAndRollsBackMessage() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: url)
        let workspace = try await store.create(name: "测试")
        let thread = try await store.createThread(workspaceID: workspace.id, title: "发送")
        let draft = WorkThreadDraft(text: "不能丢失")
        try await store.saveMessageDraft(draft, threadID: thread.id)
        _ = try await store.pendingThreadDeliveries()
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &db), SQLITE_OK, "打开独立测试库")
        defer { sqlite3_close(db) }
        XCTAssertEqual(sqlite3_exec(db, "CREATE TRIGGER fail_outbox BEFORE INSERT ON thread_outbox BEGIN SELECT RAISE(ABORT,'test failure'); END;", nil, nil, nil), SQLITE_OK, "安装原子性失败点")
        do {
            _ = try await store.appendMessage(threadID: thread.id, id: UUID().uuidString, text: draft.text, clearingDraftRevision: draft.revision)
            XCTFail("待发记录写失败不能报告发送成功")
        } catch { XCTAssertTrue(error is WorkThreadError, "SQLite 失败应明确返回本机存储错误") }
        let messages = try await store.messages(threadID: thread.id)
        let drafts = try await store.messageDrafts()
        XCTAssertTrue(messages.isEmpty, "同一事务必须回滚消息")
        XCTAssertEqual(drafts[thread.id], draft, "写失败不能清空或替换输入草稿")
    }
}

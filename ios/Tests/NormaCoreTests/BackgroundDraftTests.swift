import XCTest
@testable import NormaCore

@MainActor final class BackgroundDraftTests: XCTestCase {
    /// 后台恢复的原卡已有历史会话、尚无运行进程时，短消息不能被误判为未保存。
    ///
    /// 数据构造：原卡 7，run_id=null、session_id=saved-session；hi 为 2 个 UTF-8 字节，
    ///   远低于草稿上限 64 × 1024 = 65536 字节；另一原卡 8 = 7 + 1 用于隔离对照。
    /// 执行过程：按真实快照生成草稿范围 → 编辑 hi 并保存 → 保存另一卡 → 重建草稿库 → 确认发送回执。
    /// 预期结果：两卡草稿均落盘、无保存失败；新运行不自动继承草稿；错会话回执拒绝，准确成功才清空原稿。
    func testSavedSessionWithoutRunningProcessCanPersistShortDraft() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("drafts.json")
        let card = try JSONDecoder().decode(CardSnapshot.self, from: Data(#"{"node_id":7,"profile":"Claude","title":"Claude","mobile_control":true,"state":"idle","run_id":null,"session_id":"saved-session","turn_id":null,"task_title":null}"#.utf8))
        let machineID = "mac-" + String(repeating: "a", count: 64)
        let scope = ChatDraft.Scope(machineID: machineID, workspaceID: "project-a", routeID: "route-a",
                                    nodeID: card.id, runID: card.runID, sessionID: card.sessionID)
        let other = ChatDraft.Scope(machineID: machineID, workspaceID: "project-a", routeID: "route-a",
                                    nodeID: 8, runID: "other-run", sessionID: "other-session")
        let book = ChatDraftBook(store: ChatDraftStore(url: url)); await book.load()
        book.update("hi", for: scope)
        do { try await book.flush() }
        catch { XCTFail("后台原卡的短消息必须落盘，实际错误：\(book.failure ?? error.localizedDescription)"); return }
        XCTAssertNil(book.failure, "保留历史 session 的卡片不能触发全局草稿保存失败")
        book.update("other", for: other)
        try await book.flush()
        let reopened = ChatDraftBook(store: ChatDraftStore(url: url)); await reopened.load()
        XCTAssertEqual(reopened.draft(for: scope)?.text, "hi", "未启动原卡的短草稿必须完整恢复")
        XCTAssertEqual(reopened.draft(for: other)?.text, "other", "其他卡片不能被前一张卡的保存错误阻断")
        let running = ChatDraft.Scope(machineID: machineID, workspaceID: "project-a", routeID: "route-a",
                                      nodeID: card.id, runID: "new-run", sessionID: card.sessionID)
        XCTAssertNil(reopened.draft(for: running), "运行身份改变后不能静默迁移旧草稿")
        let draft = try XCTUnwrap(reopened.draft(for: scope), "发送必须绑定已保存的准确草稿版本")
        let command = try PendingMobileCommand(machineID: machineID, deviceID: "phone-a", workspaceID: "project-a",
                                               routeID: "route-a", revision: 1, sending: card, text: draft.text,
                                               draftID: draft.id, operationID: "message-a")
        let receiptJSON = #"{"device_id":"phone-a","operation_id":"message-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7,"run_id":"new-run","session_id":"saved-session","turn_id":"turn-a"},"error":null}"#
        let wrong = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(receiptJSON.replacingOccurrences(of: "saved-session", with: "wrong-session").utf8))
        do { try await reopened.confirm(command, receipt: wrong); XCTFail("错误会话回执必须被拒绝") } catch {}
        XCTAssertEqual(reopened.draft(for: scope)?.text, "hi", "错误会话回执不能清除未确认的原文")
        let receipt = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(receiptJSON.utf8))
        try await reopened.confirm(command, receipt: receipt)
        let disk = try await ChatDraftStore(url: url).load()
        XCTAssertFalse(disk.contains { $0.scope == scope }, "首次发送成功后原稿必须从磁盘清空")
        XCTAssertTrue(disk.contains { $0.scope == other && $0.text == "other" }, "另一个原卡的草稿不能随回执删除")
    }
}

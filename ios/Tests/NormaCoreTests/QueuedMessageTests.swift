import XCTest
@testable import NormaCore

final class QueuedMessageTests: XCTestCase {
    /// 入队是持久化交接，仍须独立查询发送状态；结果绑定完整原文与来源，不能认领旧回合。
    ///
    /// 数据构造：node7/run-a/session-a 的忙碌 Claude；消息含LF/Tab，一份原草稿和后来的不同版本。
    /// 执行过程：保存排队请求 → 编解码 → 验证入队回执 → 清原草稿 → 查询queued/dispatching/sent/cancelled/unknown → 错身份拒绝。
    /// 预期结果：排队类型和正文原样保留；入队不报Provider已收到；仅新回合真实结果有效，其他来源/正文/未知伪成功均拒绝。
    @MainActor func testQueueHandoffAndTerminalReceiptsPreserveIdentity() async throws {
        let raw = #"{"node_id":7,"profile":"Claude","title":"任务","mobile_control":true,"state":"thinking","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","task_title":null}"#
        let card = try JSONDecoder().decode(CardSnapshot.self, from: Data(raw.utf8))
        let machine = "mac-" + String(repeating: "a", count: 64)
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let book = ChatDraftBook(store: ChatDraftStore(url: directory.appendingPathComponent("drafts.json")))
        await book.load()
        let scope = ChatDraft.Scope(machineID: machine, workspaceID: "project-a", routeID: "route-a", nodeID: 7, runID: "run-a", sessionID: "session-a")
        let text = "next\n\tstep"
        book.update(text, for: scope); try await book.flush()
        let draft = try XCTUnwrap(book.draft(for: scope), "原草稿")
        let pending = try PendingMobileCommand(machineID: machine, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 2, queuing: card, text: text, draftID: draft.id, operationID: "queued-a")
        let invalidSession = try JSONDecoder().decode(CardSnapshot.self, from: Data(raw.replacingOccurrences(of: "session-a", with: "").utf8))
        XCTAssertThrowsError(try PendingMobileCommand(machineID: machine, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 2, queuing: invalidSession, text: text), "缺失会话不能持久化为可提交队列")
        let unsupported = try JSONDecoder().decode(CardSnapshot.self, from: Data(raw.replacingOccurrences(of: "Claude", with: "OpenCode").utf8))
        XCTAssertThrowsError(try PendingMobileCommand(machineID: machine, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 2, queuing: unsupported, text: text), "尚未支持的Provider不能提交手机队列")
        let store = PendingCommandStore(url: directory.appendingPathComponent("commands.json"))
        try await store.save(pending)
        let loaded = try await store.load()
        XCTAssertEqual(loaded, [pending], "排队记录需支持重启后仅回查")
        let encoded = String(decoding: try JSONEncoder().encode(pending.request), as: UTF8.self)
        XCTAssertTrue(encoded.contains("queue_message"), "必须使用排队类型")
        XCTAssertFalse(encoded.contains("send_message"), "不得退回覆盖当前任务的直接发送")
        let ack = #"{"device_id":"phone-a","operation_id":"queued-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7,"run_id":"run-a","session_id":"session-a","queue_id":"queued-a","status":"queued"},"error":null}"#
        let receipt = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(ack.utf8))
        try receipt.validate(for: pending)
        XCTAssertEqual(receipt.result?.queueID, "queued-a", "交接须绑定同一条队列")
        let wrongAck = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(ack.replacingOccurrences(of: #""queue_id":"queued-a""#, with: #""queue_id":"queued-b""#).utf8))
        XCTAssertThrowsError(try wrongAck.validate(for: pending), "另一条入队回执不能清除本条草稿")
        try await book.confirm(pending, receipt: receipt)
        XCTAssertNil(book.draft(for: scope), "准确入队回执证明原文已持久化，可完成草稿交接")
        book.update(text, for: scope); try await book.flush()
        try await book.confirm(pending, receipt: receipt)
        XCTAssertNotNil(book.draft(for: scope), "重复入队回执不得清除后来同文草稿")
        let status = #"{"device_id":"phone-a","operation_id":"queued-a","workspace_id":"project-a","route_id":"route-a","node_id":7,"profile":"Claude","run_id":"run-a","session_id":"session-a","text":"next\n\tstep","state":"queued","result":null,"error":null}"#
        func read(_ raw: String) throws -> QueuedMessageStatus {
            let value = try JSONDecoder().decode(QueuedMessageStatus.self, from: Data(raw.utf8))
            try value.validate(for: pending); return value
        }
        XCTAssertEqual(try read(status).state, "queued", "queued表示等待，不是已发送")
        XCTAssertEqual(try read(status.replacingOccurrences(of: #""state":"queued""#, with: #""state":"dispatching""#)).state, "dispatching", "领取只代表正在提交")
        let success = status.replacingOccurrences(of: #""state":"queued""#, with: #""state":"sent""#).replacingOccurrences(of: #""result":null"#, with: #""result":{"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-b"}"#)
        XCTAssertEqual(try read(success).result?.turnID, "turn-b", "发送需要新回合证据")
        XCTAssertThrowsError(try read(success.replacingOccurrences(of: "turn-b", with: "turn-a")), "不得拿当前旧回合冒充新消息")
        XCTAssertThrowsError(try read(success.replacingOccurrences(of: "session-a", with: "session-b")), "另一会话不可认领")
        XCTAssertThrowsError(try read(status.replacingOccurrences(of: "phone-a", with: "phone-b")), "另一设备不可认领")
        XCTAssertThrowsError(try read(status.replacingOccurrences(of: "route-a", with: "route-b")), "另一窗口不可认领")
        XCTAssertThrowsError(try read(status.replacingOccurrences(of: #"next\n\tstep"#, with: "changed")), "正文不一致不得静默接受")
        XCTAssertThrowsError(try read(status.replacingOccurrences(of: #""state":"queued""#, with: #""state":"sent""#)), "没有Provider结果不能显示已发送")
        let cancelled = status.replacingOccurrences(of: #""state":"queued""#, with: #""state":"cancelled""#).replacingOccurrences(of: #""error":null"#, with: #""error":"cancelled""#)
        XCTAssertTrue(try read(cancelled).isTerminal, "已撤回是终态")
        XCTAssertEqual(try read(cancelled).text, text, "撤回后原文仍可找回")
        XCTAssertFalse(try read(status).isTerminal, "等待中不能当作完成清除")
        let unknown = status.replacingOccurrences(of: #""state":"queued""#, with: #""state":"outcome_unknown""#).replacingOccurrences(of: #""error":null"#, with: #""error":"outcome_unknown""#)
        XCTAssertEqual(try read(unknown).state, "outcome_unknown", "未知结果必须独立保留")
        XCTAssertThrowsError(try read(unknown.replacingOccurrences(of: #""error":"outcome_unknown""#, with: #""error":null"#)), "未知状态不能丢失不确定标记")
    }
}

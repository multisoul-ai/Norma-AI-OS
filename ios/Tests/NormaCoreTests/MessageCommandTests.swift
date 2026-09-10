import XCTest
@testable import NormaCore

final class MessageCommandTests: XCTestCase {
    /// 手机发送必须保存原文和准确卡片；成功回执包含真实运行/会话/回合，不能用普通node回执冒充。
    ///
    /// 数据构造：32 KiB = 32768 UTF-8 字节上限，空卡7和已绑定run-a/session-a；换行允许，ESC不允许。
    /// 执行过程：创建发送并编码 → 保存/重读 → 校验冷启动回执 → 缺回合拒绝 → 旧会话发送与错run → 超限/控制字符。
    /// 预期结果：原文准确恢复；普通卡片回执和错run拒绝；空文本、超限及终端控制字符不可发送。
    func testMessageIdentityPersistenceAndReceipt() async throws {
        let card = try JSONDecoder().decode(CardSnapshot.self, from: Data(#"{"node_id":7,"profile":"Claude","title":"Agent","mobile_control":true,"state":"idle","run_id":null,"session_id":null,"turn_id":null,"task_title":null}"#.utf8))
        let machine = "mac-" + String(repeating: "a", count: 64)
        let pending = try PendingMobileCommand(machineID: machine, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 1, sending: card, text: "任务\n保留换行", operationID: "message-a")
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = PendingCommandStore(url: dir.appendingPathComponent("commands.json"))
        try await store.save(pending)
        let saved = try await store.load()
        XCTAssertEqual(saved, [pending], "重启后只回查这条原始发送，不生成新ID")
        let receiptJSON = #"{"device_id":"phone-a","operation_id":"message-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-a"},"error":null}"#
        let result = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(receiptJSON.utf8))
        try result.validate(for: pending)
        let missing = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(receiptJSON.replacingOccurrences(of: #","turn_id":"turn-a""#, with: "").utf8))
        XCTAssertThrowsError(try missing.validate(for: pending), "只有node ID不能证明消息已送达Provider")
        let active = try JSONDecoder().decode(CardSnapshot.self, from: Data(#"{"node_id":7,"profile":"Claude","title":"Agent","mobile_control":true,"state":"done","run_id":"run-a","session_id":"session-a","turn_id":null,"task_title":null}"#.utf8))
        let followup = try PendingMobileCommand(machineID: machine, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 1, sending: active, text: "继续", operationID: "message-a")
        let foreign = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(receiptJSON.replacingOccurrences(of: "run-a", with: "run-other").utf8))
        XCTAssertThrowsError(try foreign.validate(for: followup), "已有会话不能被另一运行回执替换")
        XCTAssertThrowsError(try PendingMobileCommand(machineID: machine, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 1, sending: card, text: String(repeating: "a", count: 32769)), "32769 = 32KiB+1，必须拒绝")
        XCTAssertThrowsError(try PendingMobileCommand(machineID: machine, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 1, sending: card, text: "\u{1b}exit"), "正文不能注入终端控制字符")
    }
    /// 整理成果期间保留会话并允许排队，禁止续发、删除或停止已经结束的任务。
    ///
    /// 数据构造：node7/run-a/session-a，状态collecting且活动turn为空；1条后续指令。
    /// 执行过程：验证快照 → 直接发送/删除/停止均尝试 → 将原文入队。
    /// 预期结果：快照合法且不伪装执行中；只有队列成功，不能破坏正在保存的成果。
    func testCollectingDeliveryPermitsOnlyQueuedMessages() throws {
        let card = try JSONDecoder().decode(CardSnapshot.self, from: Data(#"{"node_id":7,"profile":"Claude","title":"Agent","mobile_control":true,"state":"collecting","run_id":"run-a","session_id":"session-a","turn_id":null,"task_title":null}"#.utf8))
        try card.validate()
        XCTAssertFalse(card.isBusy, "整理文件不应显示正在执行工具或可停止")
        let machine = "mac-" + String(repeating: "a", count: 64)
        XCTAssertThrowsError(try PendingMobileCommand(machineID: machine, deviceID: "phone", workspaceID: "project", routeID: "route", revision: 1, sending: card, text: "继续"), "整理完成前不可续发")
        XCTAssertThrowsError(try PendingMobileCommand(machineID: machine, deviceID: "phone", workspaceID: "project", routeID: "route", revision: 1, deleting: card, confirmBusy: true), "整理期间不可删除")
        XCTAssertThrowsError(try PendingMobileCommand(machineID: machine, deviceID: "phone", workspaceID: "project", routeID: "route", revision: 1, interrupting: card), "不能中断一个没有活动turn的任务")
        let queued = try PendingMobileCommand(machineID: machine, deviceID: "phone", workspaceID: "project", routeID: "route", revision: 1, queuing: card, text: "下一步")
        XCTAssertEqual(queued.request.command.messageContent?.text, "下一步", "下一步可排队，保留原文等待成果固定")
    }
}

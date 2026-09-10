import XCTest
@testable import NormaCore

final class InterruptTests: XCTestCase {
    /// 停止记录绑定准确回合；真实终态与请求接受严格分开，重启不改变操作身份。
    ///
    /// 数据构造：node=7、run-a/session-a/turn-a；一份停止请求和三种真实终态。
    /// 执行过程：保存并重开 → 编解码 → 检查准确及错回合回执 → 缺少终态/accepted 伪造结果 → 正在停止状态。
    /// 预期结果：请求原样保留；只接受准确回合的 interrupted/completed/failed；未知/错回合不报成功；等待中不能再次停止。
    func testInterruptPersistsAndRequiresExactTerminalEvidence() async throws {
        let raw = #"{"node_id":7,"profile":"Claude","title":"写作","mobile_control":true,"state":"thinking","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","task_title":null}"#
        func card(_ raw: String) throws -> CardSnapshot { try JSONDecoder().decode(CardSnapshot.self, from: Data(raw.utf8)) }
        func command(_ card: CardSnapshot) throws -> PendingMobileCommand {
            try PendingMobileCommand(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 2, interrupting: card, operationID: "stop-a")
        }
        let pending = try command(card(raw))
        let native = try card(raw.replacingOccurrences(of: #""mobile_control":true"#, with: #""mobile_control":true,"can_interrupt":false"#))
        XCTAssertThrowsError(try command(native), "原生终端尚无停止回执能力时不能显示可停止")
        let supported = try card(raw.replacingOccurrences(of: #""mobile_control":true"#, with: #""mobile_control":true,"can_interrupt":true"#))
        XCTAssertNoThrow(try command(supported), "已接通回执的原生Claude应复用停止命令")
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).appendingPathComponent("pending.json")
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        try await PendingCommandStore(url: url).save(pending)
        let restored = try await PendingCommandStore(url: url).load()
        XCTAssertEqual(restored, [pending], "重启后只能回查同一请求")
        let data = try JSONEncoder().encode(pending.request)
        XCTAssertTrue(String(decoding: data, as: UTF8.self).contains("interrupt_turn"), "必须使用停止协议类型")
        XCTAssertFalse(String(decoding: data, as: UTF8.self).contains("send_message"), "停止不能变成发消息")
        let receipt = #"{"device_id":"phone-a","operation_id":"stop-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-a","status":"interrupted"},"error":null}"#
        func read(_ text: String) throws -> MobileCommandReceipt {
            let value = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(text.utf8))
            try value.validate(for: pending); return value
        }
        XCTAssertEqual(try read(receipt).result?.status, "interrupted", "准确证据才确认已停止")
        XCTAssertEqual(try read(receipt.replacingOccurrences(of: "interrupted", with: "completed")).result?.status, "completed", "自然完成不能伪装成中断")
        XCTAssertEqual(try read(receipt.replacingOccurrences(of: "interrupted", with: "failed")).result?.status, "failed", "任务失败必须如实显示")
        XCTAssertThrowsError(try read(receipt.replacingOccurrences(of: "turn-a", with: "turn-b")), "另一回合证据不可接受")
        XCTAssertThrowsError(try read(receipt.replacingOccurrences(of: "session-a", with: "session-b")), "另一会话证据不可接受")
        XCTAssertThrowsError(try read(receipt.replacingOccurrences(of: "run-a", with: "run-b")), "重开的运行不能认领旧停止")
        XCTAssertThrowsError(try read(receipt.replacingOccurrences(of: "interrupted", with: "running")), "非终态不可作为成功")
        XCTAssertThrowsError(try read(receipt.replacingOccurrences(of: #","status":"interrupted""#, with: "")), "缺终态不可报告成功")
        XCTAssertThrowsError(try read(receipt.replacingOccurrences(of: "succeeded", with: "accepted")), "接收请求不能携带完成结果")
        let stopping = try card(raw.replacingOccurrences(of: "thinking", with: "interrupting"))
        try stopping.validate()
        XCTAssertTrue(stopping.isBusy, "正在停止仍是忙碌状态")
        XCTAssertThrowsError(try command(stopping), "等待中不能创建重复停止")
        XCTAssertThrowsError(try command(card(raw.replacingOccurrences(of: "thinking", with: "idle"))), "空闲卡片不可停止")
        XCTAssertThrowsError(try command(card(raw.replacingOccurrences(of: #""turn-a""#, with: "null"))), "没有回合身份不可停止")
    }
}

import XCTest
@testable import NormaCore

final class TerminalTests: XCTestCase {
    /// 竖屏只重排已确认的软折行，旧服务、真实换行和增量更新仍保持准确。
    /// 数据构造：8列、4行；hello后3个空格占满首行；world后显式换行，空行后hard。
    /// 执行过程：解码旧协议 → 加软折行标记 → 只更新标记的流事件 → 注入错误标记长度。
    /// 预期结果：旧协议不猜测换行；新协议保留空格/空行；增量可恢复硬换行；异常清除可输入页。
    func testPhoneReflowPreservesHardBreaksAndLegacyScreens() throws {
        let scope = TerminalRequest(workspaceID: "project-a", routeID: "route-a", nodeID: 7, runID: "run-a", sessionID: nil)
        var body: [String: Any] = ["card": ["node_id": 7, "profile": "Codex", "title": "Codex", "mobile_control": true,
            "state": "ready", "run_id": "run-a"], "revision": 1, "screen_signature": 123, "input_sequence": 0,
            "start_line": 0, "total_lines": 4, "cols": 8, "running": true, "input_ready": true,
            "rows": ["hello   ", "world", "", "hard"]]
        let legacy = try JSONDecoder().decode(TerminalPage.self, from: JSONSerialization.data(withJSONObject: body))
        XCTAssertEqual(legacy.text(), "hello   \nworld\n\nhard", "旧服务无标记，不能猜测并合并硬换行")
        body["soft_wrapped"] = [true, false, false, false]
        let request = try JSONSerialization.jsonObject(with: JSONEncoder().encode(scope))
        func event(_ sequence: Int, _ from: Int = 0) throws -> TerminalEvent {
            try JSONDecoder().decode(TerminalEvent.self, from: JSONSerialization.data(withJSONObject: [
                "stream_id": "stream-a", "sequence": sequence, "request": request,
                "page": body, "replace_from": from, "remove_count": 0]))
        }
        var timeline = TerminalTimeline(request: scope)
        let initial = try XCTUnwrap(timeline.apply(event(0)))
        XCTAssertEqual(initial.text(), "hello   world\n\nhard", "软折行必须合并，显式空行必须保留")
        XCTAssertFalse(initial.text().contains("hello   \nworld"), "竖屏不能残留电脑的软折行")
        XCTAssertEqual(initial.text(lastRows: 3), "world\n\nhard", "预览截取不能拼接未接收的上一页")
        body["rows"] = [String](); body["soft_wrapped"] = [false, false, false, false]
        XCTAssertEqual(try timeline.apply(event(1, 4))?.text(), legacy.text(), "只改折行标记也必须更新正文")
        body["soft_wrapped"] = [true]
        XCTAssertThrowsError(try timeline.apply(event(2, 4)), "非空标记必须覆盖合并后的全部行")
        XCTAssertNil(timeline.page, "无效页不能保留可输入状态")
    }

    /// 终端增量只更新准确行，旧会话/序号缺口必须清空可操作状态。
    /// 数据构造：2行首屏，sequence=0；第二行替换sequence=1；重复旧事件sequence=0。
    /// 执行过程：首屏→增量→heartbeat→缺口→重新首屏；验证终端命令的编码和回执。
    /// 预期结果：保留第一行、替换第二行；不重复历史；错会话/缺口不保留可输入屏幕。
    func testTerminalDeltasAndExactInputReceipt() throws {
        let card: [String: Any] = ["node_id": 7, "profile": "Codex", "title": "Codex", "mobile_control": true,
            "state": "ready", "run_id": "run-a", "session_id": "session-a"]
        let scope = TerminalRequest(workspaceID: "project-a", routeID: "route-a", nodeID: 7, runID: "run-a", sessionID: "session-a")
        let page: [String: Any] = ["card": card, "revision": 1, "screen_signature": 123, "input_sequence": 0, "start_line": 0,
            "total_lines": 2, "cols": 80, "running": true, "input_ready": true, "rows": ["old", "draft"]]
        let request = try JSONSerialization.jsonObject(with: JSONEncoder().encode(scope))
        func event(_ sequence: Int, _ body: [String: Any]?, _ from: Int = 0, _ count: Int = 0) throws -> TerminalEvent {
            try JSONDecoder().decode(TerminalEvent.self, from: JSONSerialization.data(withJSONObject:
                ["stream_id": "stream-a", "sequence": sequence, "request": request, "page": body as Any? ?? NSNull(), "replace_from": from, "remove_count": count]))
        }
        var timeline = TerminalTimeline(request: scope)
        let initial = try event(0, page)
        XCTAssertEqual(try timeline.apply(initial)?.rows, ["old", "draft"], "首屏必须完整")
        var delta = page; delta["rows"] = ["new"]; delta["revision"] = 2
        XCTAssertEqual(try timeline.apply(event(1, delta, 1, 1))?.rows, ["old", "new"], "增量不得追加重复屏幕")
        XCTAssertNil(try timeline.apply(event(1, nil)), "空闲心跳不能重新渲染正文")
        XCTAssertThrowsError(try timeline.apply(initial), "旧序号不能退回可输入屏幕")
        XCTAssertNil(timeline.page, "缺口必须清空操作状态")
        _ = try timeline.apply(initial)
        let current = try XCTUnwrap(timeline.page)
        let command = try PendingMobileCommand(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone-a",
            workspaceID: "project-a", routeID: "route-a", revision: 1, terminal: current, input: .enter, operationID: "operation-a")
        let encoded = try JSONEncoder().encode(command)
        XCTAssertEqual(try JSONDecoder().decode(PendingMobileCommand.self, from: encoded), command, "重启后仍可回查同一按键")
        let wire = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
        XCTAssertTrue(String(decoding: encoded, as: UTF8.self).contains("terminal_input"), "必须编码为原终端输入")
        XCTAssertNotNil(wire["request"], "持久化必须保留请求目标")
        var result: [String: Any] = ["node_id": 7, "run_id": "run-a", "session_id": "session-a", "status": "written"]
        func receipt(_ result: [String: Any]) throws -> MobileCommandReceipt {
            try JSONDecoder().decode(MobileCommandReceipt.self, from: JSONSerialization.data(withJSONObject: [
                "device_id": "phone-a", "operation_id": "operation-a", "workspace_id": "project-a", "route_id": "route-a",
                "state": "succeeded", "result": result]))
        }
        XCTAssertNoThrow(try receipt(result).validate(for: command), "written只能表示原PTY写入")
        result["status"] = "completed"
        XCTAssertThrowsError(try receipt(result).validate(for: command), "不能把任务完成当作终端写入回执")
        result["status"] = "written"; result["session_id"] = "other-session"
        XCTAssertThrowsError(try receipt(result).validate(for: command), "回执不得来自另一个会话")

        XCTAssertThrowsError(try TerminalInput.text("bad\rcommand").validate(), "CR不能混入正文代替明确Enter")
        var wrong = page; wrong["card"] = card.merging(["run_id": "run-b"]) { _, b in b }
        XCTAssertThrowsError(try timeline.apply(event(1, wrong, 0, 2)), "新run不能接管旧页")
        XCTAssertNil(timeline.page, "错run不得继续输入")
    }
}

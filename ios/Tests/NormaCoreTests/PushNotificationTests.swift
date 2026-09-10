import XCTest
@testable import NormaCore

final class PushNotificationTests: XCTestCase {
    /// 通知仅提供定位线索：严格解析身份，并拒绝旧窗口、换运行和已经变化的回合。
    ///
    /// 数据构造：节点7、run-a/session-a/turn-a；64位机器指纹；旧窗口route-old、旧回合turn-old。
    /// 执行过程：解析完成通知 → 匹配真实项目卡片 → 替换路由/回合 → 拒绝非法数字与审批缺失ID。
    /// 预期结果：当前任务可以定位；旧目标不能落到新任务，通知不能携带自动批准或地址。
    func testNotificationTargetRequiresExactLiveIdentity() throws {
        let body: [String: Any] = ["version": 1, "event_id": "00000000-0000-4000-8000-000000000001",
            "kind": "completed", "machine_id": "mac-" + String(repeating: "a", count: 64), "workspace_id": "workspace-a",
            "route_id": "route-a", "node_id": "7", "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a"]
        let target = try PushNotificationTarget.decode(JSONSerialization.data(withJSONObject: body))
        let projectJSON = #"{"workspace_id":"workspace-a","display_name":"A","state":"open","route_id":"route-a","card_count":1,"cards":[{"node_id":7,"profile":"Codex","title":"Card","mobile_control":true,"state":"done","run_id":"run-a","session_id":"session-a","turn_id":"turn-a"}]}"#
        let project = try JSONDecoder().decode(ProjectSnapshot.self, from: Data(projectJSON.utf8))
        XCTAssertEqual(try target.card(in: project).id, 7, "当前已验证项目应定位准确卡片")
        let native = try JSONDecoder().decode(ProjectSnapshot.self, from: Data(projectJSON.replacingOccurrences(of: "true", with: "false").utf8))
        XCTAssertEqual(try target.card(in: native).id, 7, "原生卡未开启控制也必须可以只读定位")
        XCTAssertFalse(try target.card(in: native).mobileControl, "通知不能隐式开启手机控制")
        let replaced = try JSONDecoder().decode(ProjectSnapshot.self, from: Data(projectJSON.replacingOccurrences(of: "route-a", with: "route-old").utf8))
        XCTAssertThrowsError(try target.card(in: replaced), "旧窗口通知不能打开同名新窗口")
        let next = try JSONDecoder().decode(ProjectSnapshot.self, from: Data(projectJSON.replacingOccurrences(of: "turn-a", with: "turn-new").utf8))
        XCTAssertThrowsError(try target.card(in: next), "回合已变更必须明确过期，不能指向新任务")
        var invalid = body; invalid["node_id"] = "18446744073709551616" // u64.max + 1, never truncate to a different node.
        XCTAssertThrowsError(try PushNotificationTarget.decode(JSONSerialization.data(withJSONObject: invalid)), "节点溢出必须拒绝")
        invalid = body; invalid["kind"] = "approval"
        XCTAssertThrowsError(try PushNotificationTarget.decode(JSONSerialization.data(withJSONObject: invalid)), "审批事件必须指向具体请求")
        invalid["request_id"] = String(repeating: "a", count: 256)
        XCTAssertEqual(try PushNotificationTarget.decode(JSONSerialization.data(withJSONObject: invalid)).requestID?.count, 256, "应保留Provider允许的完整请求ID")
        let approvalHint = try PushNotificationTarget.decode(JSONSerialization.data(withJSONObject: invalid))
        let waiting = try JSONDecoder().decode(ApprovalSnapshot.self, from: JSONSerialization.data(withJSONObject: [
            "target":["node_id":7,"profile":"Codex","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","request_id":String(repeating: "a", count: 256)],
            "card_title":"Card", "tool":"native_terminal", "details":[:], "decisions":[]
        ] as [String: Any]))
        XCTAssertTrue(approvalHint.matches(waiting), "电脑处理的确认也应能被通知定位")
        XCTAssertTrue(waiting.decisions.isEmpty, "可定位不等于手机有批准选项")
        XCTAssertFalse(target.matches(waiting), "完成通知不能被当作审批定位")
        invalid["request_id"] = String(repeating: "a", count: 257) // 256 + 1 exceeds the existing Provider boundary.
        XCTAssertThrowsError(try PushNotificationTarget.decode(JSONSerialization.data(withJSONObject: invalid)), "不能超过Provider请求ID的长度边界")
        invalid = body; invalid["action"] = "approve"
        XCTAssertThrowsError(try PushNotificationTarget.decode(JSONSerialization.data(withJSONObject: invalid)), "通知不得携带控制动作")
        invalid = body; invalid["endpoint"] = "wss://other.example/mobile"
        XCTAssertThrowsError(try PushNotificationTarget.decode(JSONSerialization.data(withJSONObject: invalid)), "通知不得改写已配对电脑的网络地址")
    }
}

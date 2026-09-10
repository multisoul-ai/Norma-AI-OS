import XCTest
@testable import NormaCore

final class ApprovalFeedTests: XCTestCase {
    /// 电脑级完整审批基线按已授权项目绑定，序号缺口必须清空可操作旧数据。
    ///
    /// 数据构造：手机phone获A/control与C/read；A的一项审批来自node7/run-a，C已关闭。
    ///   序号0为初始完整快照，0心跳不带正文，1表示实际清空，跳到3应因缺少2失败。
    /// 执行过程：基线→心跳→清空→缺口→另一手机/未授权项目/关闭项目携带审批分别拒绝。
    /// 预期结果：初始准确审批存在且心跳保留；清空与失败后不残留旧请求，不把只读或跨来源数据变成可操作。
    func testFeedRejectsGapsAndForeignProjectContent() throws {
        let grant: [String: Any] = ["device_id":"phone", "device_name":"Phone", "revision":1, "revoked":false,
            "projects":[["workspace_id":"a", "access":"control"], ["workspace_id":"c", "access":"read"]]]
        let item: [String: Any] = ["target":["node_id":7,"profile":"Claude","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","request_id":"request-a"],
            "card_title":"Agent", "tool":"Bash", "details":["command":"printf exact"], "decisions":["deny","allow"]]
        let a: [String: Any] = ["workspace_id":"a", "display_name":"A", "state":"open", "route_id":"route-a", "card_count":1]
        let c: [String: Any] = ["workspace_id":"c", "display_name":"C", "state":"closed"]
        var body: [String: Any] = ["grant":grant,"projects":[["project":a,"items":[item]], ["project":c,"items":[]]]]
        func event(_ sequence: Int, _ body: [String: Any]?, stream: String = "feed-a") throws -> ApprovalFeedEvent {
            try JSONDecoder().decode(ApprovalFeedEvent.self, from: JSONSerialization.data(withJSONObject:
                ["stream_id":stream,"sequence":sequence,"snapshot":body as Any? ?? NSNull()]))
        }
        var timeline = ApprovalFeedTimeline(deviceID: "phone")
        let baseline = try timeline.apply(event(0, body))
        XCTAssertEqual(baseline.projects[0].items[0].target.nodeID, 7, "必须保留准确卡片身份")
        XCTAssertEqual(baseline.grant.projects[1].access, "read", "订阅不得提升只读权限")
        XCTAssertTrue(baseline.projects[1].items.isEmpty, "关闭项目不能出现审批")
        XCTAssertEqual(try timeline.apply(event(0, nil)).projects[0].items.count, 1, "无正文心跳保留已验证基线")
        body["projects"] = [["project":a,"items":[]], ["project":c,"items":[]]]
        XCTAssertTrue(try timeline.apply(event(1, body)).projects[0].items.isEmpty, "清空后不能残留旧请求")
        XCTAssertThrowsError(try timeline.apply(event(3, body)), "序号缺口必须失败")
        XCTAssertNil(timeline.snapshot, "缺口必须清理旧可操作状态")
        XCTAssertThrowsError(try timeline.apply(event(0, nil)), "不能只靠心跳重建基线")
        var foreign = grant; foreign["device_id"] = "other-phone"; body["grant"] = foreign
        XCTAssertThrowsError(try timeline.apply(event(0, body)), "其他手机授权不能混入当前连接")
        body["grant"] = grant
        body["projects"] = [["project":a,"items":[]], ["project":c,"items":[item]]]
        XCTAssertThrowsError(try timeline.apply(event(0, body)), "关闭项目的工具输入必须拒绝")
        body["projects"] = [["project":a,"items":[item,item]], ["project":c,"items":[]]]
        XCTAssertThrowsError(try timeline.apply(event(0, body)), "重复请求不能生成两张决策卡")
        var privateProject = c; privateProject["workspace_id"] = "private"
        body["projects"] = [["project":a,"items":[]], ["project":privateProject,"items":[]]]
        XCTAssertThrowsError(try timeline.apply(event(0, body)), "未授权项目不能替换C")
        body["projects"] = [["project":a,"items":[]], ["project":c,"items":[]]]
        _ = try timeline.apply(event(0, body))
        XCTAssertThrowsError(try timeline.apply(event(1, body, stream: "other-stream")), "流身份变化须重连，不能沿用旧序号")
        XCTAssertNil(timeline.snapshot, "换流失败后也必须清空旧状态")
        _ = try timeline.apply(event(0, body))
        var downgraded = grant; downgraded["revision"] = 2
        downgraded["projects"] = [["workspace_id":"a","access":"read"],["workspace_id":"c","access":"read"]]
        body["grant"] = downgraded
        XCTAssertThrowsError(try timeline.apply(event(1, body)), "同一流不能偷偷替换授权版本，必须重连建立基线")
        XCTAssertNil(timeline.snapshot, "授权变化不能保留旧control内容")
    }
}

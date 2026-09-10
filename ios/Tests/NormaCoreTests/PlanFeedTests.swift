import XCTest
@testable import NormaCore

final class PlanFeedTests: XCTestCase {
    /// 最新计划允许空态→首个计划→新计划，固定ID订阅不允许借此换计划。
    /// 数据构造：同project-a，plan-a序号1、plan-b序号2；流序号0/1/2，另注入旧计划回退与改写正文。
    /// 执行过程：空基线及心跳→首个方向→新方向→旧方向回退；新流再验证同ID内容不可改写。
    /// 预期结果：空态不是断流，最新计划正确替换，旧计划/空态回退/改写必须清空并报错。
    func testLatestPlanStreamAcceptsEmptyThenNewPlansWithoutLosingScope() throws {
        let grant: [String: Any] = ["device_id":"phone","device_name":"Phone","revision":1,"revoked":false,
                                  "projects":[["workspace_id":"project-a","access":"read"]]]
        func event(_ sequence: Int, plan: String?, ordinal: Int = 1, direction: String = "当前方向", heartbeat: Bool = false) throws -> PlanFeedEvent {
            let saved: Any = plan.map { ["sequence":ordinal,"plan_id":$0,"workspace_id":"project-a","route_id":"route-a","direction":direction,"created_at_unix":1,"draft":["summary":"工作简报","todos":[]]] as [String: Any] } ?? NSNull()
            let body: [String: Any] = ["grant":grant,"plan":saved,"decisions":[]]
            return try JSONDecoder().decode(PlanFeedEvent.self, from: JSONSerialization.data(withJSONObject:
                ["workspace_id":"project-a","plan_id":NSNull(),"stream_id":"stream-a","sequence":sequence,"snapshot":heartbeat ? NSNull() as Any : body]))
        }
        var latest = PlanFeedTimeline(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone", workspaceID: "project-a", planID: nil)
        XCTAssertNil(try latest.apply(event(0, plan: nil)), "空项目应建立有效基线")
        XCTAssertNil(try latest.apply(event(0, plan: nil, heartbeat: true)), "空态心跳不得产生假计划")
        XCTAssertEqual(try latest.apply(event(1, plan: "plan-a"))?.plan.id, "plan-a", "无需重连应发现第一份计划")
        XCTAssertEqual(try latest.apply(event(2, plan: "plan-b", ordinal: 2))?.plan.id, "plan-b", "更晚序号应替换当前方向")
        XCTAssertThrowsError(try latest.apply(event(3, plan: "plan-a")), "最新方向不得回退到旧序号")
        XCTAssertNil(latest.progress, "错误状态不得残留旧待办")
        _ = try latest.apply(event(0, plan: "plan-a"))
        XCTAssertThrowsError(try latest.apply(event(1, plan: "plan-a", direction: "被改写的方向")), "同一已保存计划不可被偷偷改写")
        _ = try latest.apply(event(0, plan: "plan-a"))
        XCTAssertThrowsError(try latest.apply(event(1, plan: nil)), "不能把已有计划误报为从未规划")
        XCTAssertNil(latest.progress, "空态回退不能继续显示旧内容")
        var fixed = PlanFeedTimeline(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone", workspaceID: "project-a", planID: "plan-a")
        XCTAssertThrowsError(try fixed.apply(event(0, plan: "plan-a")), "最新订阅事件不能混入固定计划流")
    }

    /// 计划订阅按手机/项目/计划和授权版本隔离，心跳不覆盖缓冲中的正文。
    /// 数据构造：phone以read读取project-a/plan-a，初始sequence0，变化1，缺口3（缺少2）。
    /// 执行过程：基线→无正文心跳→另一手机拒绝→缺口；再分别注入错流、错项目、错手机和新授权版本。
    /// 预期结果：拒绝决定准确显示且无执行终态；心跳不发布重复快照，任何验证失败清空旧进度。
    func testScopedPlanStreamRejectsGapsAndIgnoresHeartbeatContent() throws {
        let raw = #"{"sequence":1,"plan_id":"plan-a","workspace_id":"project-a","route_id":"route-a","direction":"验证登录","created_at_unix":1,"draft":{"summary":"补测试","todos":[{"target":{"node_id":7,"profile":"Codex","title":"登录","mobile_control":true,"state":"ready","run_id":"run-a","session_id":"session-a"},"title":"登录测试","instruction":"保留原文。"}]}}"#
        let plan = try JSONSerialization.jsonObject(with: Data(raw.utf8))
        var grant: [String: Any] = ["device_id":"phone","device_name":"Phone","revision":1,"revoked":false,
                                  "projects":[["workspace_id":"project-a","access":"read"]]]
        var body: [String: Any] = ["plan":plan,"decisions":[],"grant":grant]
        func event(_ sequence: Int, _ snapshot: [String: Any]?, stream: String = "stream-a", workspace: String = "project-a", plan: String = "plan-a") throws -> PlanFeedEvent {
            try JSONDecoder().decode(PlanFeedEvent.self, from: JSONSerialization.data(withJSONObject:
                ["stream_id":stream,"sequence":sequence,"workspace_id":workspace,"plan_id":plan,"snapshot":snapshot as Any? ?? NSNull()]))
        }
        var timeline = PlanFeedTimeline(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone", workspaceID: "project-a", planID: "plan-a")
        XCTAssertEqual(try timeline.apply(event(0, body))?.plan.planID, "plan-a", "基线必须绑定原计划")
        XCTAssertNil(try timeline.apply(event(0, nil)), "心跳不可覆盖尚未消费的完整快照")
        XCTAssertTrue(timeline.progress?.decisions.isEmpty == true, "无变更不虚构决定")
        body["decisions"] = [["todo_index":0,"choice":["type":"reject"],"receipt":["device_id":"phone-b","operation_id":"reject-b","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":["plan_id":"plan-a","todo_index":0,"status":"declined"]]]]
        let updated = try timeline.apply(event(1, body))
        XCTAssertEqual(updated?.decisions.first?.deliveryState, "declined", "另一授权手机的决定应可见")
        XCTAssertNil(updated?.decisions.first?.outcome, "拒绝不等于执行完成")
        XCTAssertThrowsError(try timeline.apply(event(3, body)), "缺少序号2必须停止使用旧进度")
        XCTAssertNil(timeline.progress, "缺口后清空旧状态")
        XCTAssertThrowsError(try timeline.apply(event(0, nil)), "新连接不能用空心跳建立基线")
        XCTAssertThrowsError(try timeline.apply(event(0, body, workspace: "private")), "不能混入未授权项目")
        XCTAssertThrowsError(try timeline.apply(event(0, body, plan: "plan-b")), "同项目其他计划也不能混用")
        _ = try timeline.apply(event(0, body))
        XCTAssertThrowsError(try timeline.apply(event(1, body, stream: "stream-b")), "流身份更换必须重新验证")
        XCTAssertNil(timeline.progress, "换流后清空旧状态")
        grant["device_id"] = "other-phone"; body["grant"] = grant
        XCTAssertThrowsError(try timeline.apply(event(0, body)), "不能借用其他手机授权")
        grant["device_id"] = "phone"; body["grant"] = grant
        _ = try timeline.apply(event(0, body))
        grant["revision"] = 2; body["grant"] = grant
        XCTAssertThrowsError(try timeline.apply(event(1, body)), "授权变化必须重新连接")
        XCTAssertNil(timeline.progress, "新授权不能沿用旧状态")
        grant["projects"] = [["workspace_id":"private","access":"read"]]; body["grant"] = grant
        XCTAssertThrowsError(try timeline.apply(event(0, body)), "计划必须在当前授权列表内")
    }
}

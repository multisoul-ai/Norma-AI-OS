import XCTest
@testable import NormaCore

final class DeliveryFeedTests: XCTestCase {
    /// 仅旧电脑对未知读取动作的准确拒绝帧触发兼容读取，不接受错ID、撤销授权或伪造成功。
    /// 数据构造：旧serde拒绝新字段时version=1/request_id=""/error=invalid_request；变更三个关键字段。
    /// 执行过程：识别旧拒绝帧→分别注入错误版本、其他request_id和unauthorized。
    /// 预期结果：只有精确旧协议形状返回true，其他数据不能绕过常规验证。
    func testLegacyReadDetectionDoesNotWeakenResponseValidation() throws {
        func data(version: Int = 1, id: String = "", error: String = "invalid_request") throws -> Data {
            try JSONSerialization.data(withJSONObject: ["version":version,"request_id":id,"error":error])
        }
        XCTAssertTrue(WireReply.rejectsNewRead(try data()), "旧版本拒绝新增字段可回退到原只读接口")
        XCTAssertFalse(WireReply.rejectsNewRead(try data(version:2)), "版本不匹配不能回退")
        XCTAssertFalse(WireReply.rejectsNewRead(try data(id:"other")), "其他请求响应不能回退")
        XCTAssertFalse(WireReply.rejectsNewRead(try data(error:"unauthorized")), "授权撤销不能回退")
    }
    /// 成果收件箱验证项目/窗口/筛选/授权和事件连续性，心跳不会覆盖正文或伪造零待验收。
    /// 数据构造：phone对project-a只读，route-a、2份待验收；随后数量1、流序号1，再注入缺口3和错窗口。
    /// 执行过程：基线→心跳→验收更新→缺口；新基线注入其他项目、错误数量、已验收条目和授权版本变化。
    /// 预期结果：2→1可信显示，心跳无更新；未知/不一致数据清空旧状态且不能被当作0。
    func testDeliveryStreamRejectsWrongScopeCountsAndGaps() throws {
        let scope = ProjectReviewRequest(workspaceID: "project-a", routeID: "route-a")
        var grant: [String: Any] = ["device_id":"phone","device_name":"Phone","revision":1,"revoked":false,
                                  "projects":[["workspace_id":"project-a","access":"read"]]]
        let key: [String: Any] = ["node_id":7,"profile":"Codex","run_id":"run-a","session_id":"session-a","turn_id":"turn-a"]
        var item: [String: Any] = ["sequence":1,"workspace_id":"project-a","outcome":["key":key,"state":"completed"],
                                  "fingerprint":String(repeating:"a",count:64),"saved_at_unix":1,"status":"available","file_count":0]
        func event(_ sequence: Int, count: Int = 2, route: String = "route-a", heartbeat: Bool = false) throws -> DeliveryFeedEvent {
            let page: [String: Any] = ["request":["workspace_id":"project-a","route_id":route],"items":[item],"node_id":NSNull(),
                                      "next_cursor":NSNull(),"pending_only":true,"pending_count":count,"collecting":false]
            let body: [String: Any] = ["grant":grant,"deliveries":page]
            return try JSONDecoder().decode(DeliveryFeedEvent.self, from: JSONSerialization.data(withJSONObject:
                ["stream_id":"stream-a","sequence":sequence,"snapshot":heartbeat ? NSNull() as Any : body]))
        }
        var timeline = DeliveryFeedTimeline(deviceID: "phone", scope: scope, nodeID: nil, before: nil, pendingOnly: true)
        XCTAssertEqual(try timeline.apply(event(0))?.pendingCount, 2, "基线必须保留完整待验收数")
        XCTAssertNil(try timeline.apply(event(0, heartbeat:true)), "心跳不能覆盖还未显示的正文")
        XCTAssertEqual(try timeline.apply(event(1,count:1))?.pendingCount, 1, "另一端验收后应更新总数")
        XCTAssertThrowsError(try timeline.apply(event(3)), "缺少序号2不能继续沿用旧成果")
        XCTAssertNil(timeline.page, "缺口清空旧结果")
        XCTAssertThrowsError(try timeline.apply(event(0,route:"route-b")), "同项目的新窗口不能冒充原路由")
        XCTAssertThrowsError(try timeline.apply(event(0,count:0)), "列表非空不能宣称零待验收")
        _ = try timeline.apply(event(0))
        grant["revision"] = 2
        XCTAssertThrowsError(try timeline.apply(event(1)), "授权变化必须重新核验")
        XCTAssertNil(timeline.page, "新授权不可继承旧结果")
        grant["device_id"] = "other"
        XCTAssertThrowsError(try timeline.apply(event(0)), "不能使用其他手机授权")
        grant["device_id"] = "phone"
        item["outcome"] = ["key":key,"state":"failed"]
        XCTAssertThrowsError(try timeline.apply(event(0)), "失败成果不能混入待验收列表")
        item["outcome"] = ["key":key,"state":"completed"]
        item["acceptance"] = ["workspace_id":"project-a","route_id":"route-a","key":key,"fingerprint":String(repeating:"a",count:64),"device_id":"phone","operation_id":"accept","accepted_at_unix":1]
        XCTAssertThrowsError(try timeline.apply(event(0)), "已验收成果不能仍被列为待验收")
    }
}

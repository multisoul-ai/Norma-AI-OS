import XCTest
@testable import NormaCore

final class RealtimeBindingTests: XCTestCase {
    /// 豆包工具票据只能转交同媒体、同网关且未过期的短期能力，不接受重定向或GPT身份。
    /// 数据构造：now=1000秒、expires=1030秒（30秒一次性票据），媒体doubao_A，HTTPS网关。
    /// 执行过程：解析合法票据和绑定 → 改媒体/来源/过期时间/路径 → 编码Mac请求。
    /// 预期结果：豆包媒体可绑定；错误来源和过期票据拒绝；只传短期字段，不混入媒体/手机凭据。
    func testDoubaoGatewayTicketIsMediaAndOriginScoped() throws {
        let endpoint = try XCTUnwrap(URL(string: "https://voice.example/v1/session"))
        var event: [String: Any] = ["call_id":"doubao_A", "url":"wss://voice.example/v1/tools",
            "token":String(repeating: "b", count: 43), "expires_at":1030]
        func parse() throws -> RealtimeGatewayTicket {
            try RealtimeGatewayTicket(data: JSONSerialization.data(withJSONObject: event), endpoint: endpoint, callID: "doubao_A", now: 1000)
        }
        let ticket = try parse()
        let binding = try RealtimeBinding(callID: "doubao_A", workspaceID: "project-a", generation: 1)
        XCTAssertEqual(binding.callID, ticket.callID, "豆包媒体身份应准确保留")
        let encoded = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(ticket)) as? [String: Any])
        XCTAssertEqual(encoded["expires_at"] as? Int, 1030, "有效期不得被手机延长")
        XCTAssertNil(encoded["call_id"], "媒体身份由外层绑定承载，不在凭据字段中重复")
        event["call_id"] = "rtc_A"
        XCTAssertThrowsError(try parse(), "不能冒用GPT或另一媒体身份")
        event["call_id"] = "doubao_A"; event["url"] = "wss://another.example/v1/tools"
        XCTAssertThrowsError(try parse(), "不能向第三方转交临时能力")
        event["url"] = "wss://voice.example/v1/tools?token=bad"
        XCTAssertThrowsError(try parse(), "查询参数不得夹带凭据或改写目的地")
        event["url"] = "wss://voice.example/v1/tools"; event["expires_at"] = 1000
        XCTAssertThrowsError(try parse(), "当前时刻已到期的票据不能继续使用")
    }

    /// 明确切换通话项目只能使用最新快照中仍有控制权限的项目，不能把浏览权限升级为派工权限。
    /// 数据构造：同一 phone-a 的两个已授权项目，write-a 为 control，read-b 为 read；missing-c 不在快照中。
    /// 执行过程：构造已验证快照 → 选择 write-a → 尝试只读项目与缺失项目。
    /// 预期结果：返回 write-a 的真实项目元数据；read-b 和 missing-c 都不能成为新派工目标。
    func testCallTargetRequiresFreshControlPermission() throws {
        let grant = try JSONDecoder().decode(DeviceGrant.self, from: Data("""
        {"device_id":"phone-a","device_name":"Phone","revision":1,"revoked":false,
        "projects":[{"workspace_id":"write-a","access":"control"},{"workspace_id":"read-b","access":"read"}]}
        """.utf8))
        let projects = try JSONDecoder().decode([ProjectSnapshot].self, from: Data("""
        [{"workspace_id":"write-a","display_name":"可执行项目","state":"closed"},
         {"workspace_id":"read-b","display_name":"只读项目","state":"closed"}]
        """.utf8))
        let snapshot = try MachineSnapshot(grant: grant, projects: projects, deviceID: "phone-a", workspaceID: nil)
        XCTAssertEqual(try snapshot.controlProject("write-a").displayName, "可执行项目", "应返回最新快照内的真实目标")
        XCTAssertThrowsError(try snapshot.controlProject("read-b"), "浏览权限不能切换为通话派工目标")
        XCTAssertThrowsError(try snapshot.controlProject("missing-c"), "旧列表里的项目不能绕过最新授权")
    }

    /// Mac 失联时恢复媒体初始角色工具，不能保留不可执行的任务工具或生成任务回执。
    /// 数据构造：初始 session 含原指令、set_behavior 与陈旧 create_agent 定义。
    /// 执行过程：尚无媒体配置 → 保存初始配置 → 生成离线更新 → 检查允许的工具与消息类型。
    /// 预期结果：保留角色 schema/原指令，移除 Mac 工具；不结束媒体，也不伪造 function output。
    func testOfflineConfigurationRestoresOnlyTheLocalAvatarTool() throws {
        var configuration = RealtimeLocalConfiguration()
        XCTAssertNil(configuration.offlineUpdate, "未收到真实会话前不能凭空编造角色配置")
        let avatar: [String: Any] = ["type":"function", "name":"set_behavior", "parameters":["type":"object"]]
        configuration.capture(["instructions":"用中文交谈", "tools":[avatar, ["type":"function", "name":"create_agent"]]])
        let update = try XCTUnwrap(configuration.offlineUpdate, "已有媒体配置应可以在 Mac 离线后继续聊天")
        let session = try XCTUnwrap(update["session"] as? [String: Any], "恢复事件必须只更新会话能力")
        let tools = try XCTUnwrap(session["tools"] as? [[String: Any]], "必须显式移除任务工具")
        XCTAssertEqual(update["type"] as? String, "session.update", "恢复不得挂断音频或伪造任务结果")
        XCTAssertEqual(tools.count, 1, "离线时只保留本机可执行的角色工具")
        XCTAssertEqual(tools.first?["name"] as? String, "set_behavior", "角色表情仍应可用")
        XCTAssertFalse(tools.contains { $0["name"] as? String == "create_agent" }, "离线不能向模型继续暴露派工能力")
        XCTAssertTrue((session["instructions"] as? String)?.hasPrefix("用中文交谈") == true, "必须保留原媒体会话的聊天指令")
        XCTAssertNil(update["item"], "手机不能替 Mac 伪造工具回执")
    }

    /// Mac 绑定回执必须匹配原电脑、媒体会话、项目和代次，连接成功不等于工具可执行。
    /// 数据构造：rtc_A / project-a / generation=2；同一 phone-a 的 control grant revision=1。
    /// 执行过程：合法 attached → 可选采样时间换算/零值拒绝 → 旧代次 → 错项目 → 错电脑 → 只读权限。
    /// 预期结果：合法绑定可呈现但不能派工，旧 Mac 无时间保持未知；错归属回执被拒绝，不污染通话。
    func testBindingRequiresExactIdentityAndCurrentControlGrant() throws {
        let binding = try RealtimeBinding(callID: "rtc_A", workspaceID: "project-a", generation: 2)
        var result: [String: Any] = ["status":"realtime_binding", "machine_id":"mac-a", "state":"attached",
            "binding":["call_id":"rtc_A", "workspace_id":"project-a", "generation":2],
            "grant":["device_id":"phone-a", "device_name":"Phone", "projects":[["workspace_id":"project-a", "access":"control"]], "revision":1, "revoked":false]]
        func decode() throws -> RealtimeBindingStatus {
            let data = try JSONSerialization.data(withJSONObject: ["version":1,"request_id":"bind-a","result":result])
            return try RealtimeBindingStatus.decode(data, requestID: "bind-a", machineID: "mac-a", deviceID: "phone-a", binding: binding)
        }
        let status = try decode()
        XCTAssertEqual(status.state, .attached, "同会话确认应可显示 Mac 已连接")
        XCTAssertFalse(status.toolsAvailable, "sideband 连接本身不能假装任务工具已就绪")
        XCTAssertNil(status.snapshotObservedAt, "旧 Mac 没有采样时间时不能用手机当前时间伪造快照")
        // 1000 seconds × 1000 milliseconds/second = 1,000,000；换算不得使用接收时间。
        result["snapshot_observed_at_unix_ms"] = 1_000_000
        XCTAssertEqual(try decode().snapshotObservedAt, Date(timeIntervalSince1970: 1000), "必须保留 Mac 的真实采样时间")
        result["snapshot_observed_at_unix_ms"] = 0
        XCTAssertThrowsError(try decode(), "零时间为未知哨兵，不能显示为 1970 年的有效快照")
        result.removeValue(forKey: "snapshot_observed_at_unix_ms")
        result["binding"] = ["call_id":"rtc_A", "workspace_id":"project-a", "generation":1]
        XCTAssertThrowsError(try decode(), "旧连接代次必须拒绝")
        result["binding"] = ["call_id":"rtc_A", "workspace_id":"project-b", "generation":2]
        XCTAssertThrowsError(try decode(), "不能接收浏览项目的绑定回执")
        result["binding"] = ["call_id":"rtc_A", "workspace_id":"project-a", "generation":2]
        result["machine_id"] = "mac-b"
        XCTAssertThrowsError(try decode(), "另一电脑的回执不得进入当前通话")
        result["machine_id"] = "mac-a"
        result["grant"] = ["device_id":"phone-a", "device_name":"Phone", "projects":[["workspace_id":"project-a", "access":"read"]], "revision":1, "revoked":false]
        XCTAssertThrowsError(try decode(), "只读项目不得启用控制绑定")
        XCTAssertThrowsError(try RealtimeBinding(callID: "call_function_a", workspaceID: "project-a", generation: 2), "function call_id 不能混用为媒体身份")
    }
}

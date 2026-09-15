import XCTest
@testable import NormaCore

final class WireTests: XCTestCase {
    /// 六位核对码在手机独立计算；拒绝错请求响应和扩大/损坏的授权。
    ///
    /// 数据构造：与 Rust 的 JSON tuple/SHA256 相同的固定 ASCII 字段，预期值由 Python hashlib 独立算出。
    /// 执行过程：计算视觉核对码 → 校验错误 request_id → 校验撤销授权。
    /// 预期结果：码与独立参考值一致，错回执和撤销授权不能进入已连接状态。
    func testVerificationAndReceiptIdentity() throws {
        let credential = DeviceCredential(deviceID: "phone-a", token: String(repeating: "a", count: 64))
        let code = try verificationCode(offerID: String(repeating: "b", count: 64), secret: String(repeating: "c", count: 64), credential: credential)
        XCTAssertEqual(code, "011399", "必须与 Rust / 独立 SHA256 向量一致")
        let response = Data(#"{"version":1,"request_id":"wrong","result":{"status":"authorized"}}"#.utf8)
        XCTAssertThrowsError(try WireReply.decode(response, expectedID: "right"), "错请求的成功回执不能认领")
        let grant = try JSONDecoder().decode(DeviceGrant.self, from: Data(#"{"device_id":"phone-a","device_name":"iPhone","projects":[],"revision":1,"revoked":true}"#.utf8))
        XCTAssertThrowsError(try grant.validate(deviceID: "phone-a"), "撤销记录不能显示已授权")
    }

    /// 项目快照校验授权范围、窗口身份与完整卡片列表，不把关闭窗口包装成空闲运行。
    ///
    /// 数据构造：phone-a 的一个只读 project-a，route-a 内一张未运行 Codex 卡，card_count=1。
    /// 执行过程：读取合法快照 → 插入未授权项目 → 删除运行窗口身份 → 损坏卡片数量 → 改为关闭状态。
    /// 预期结果：合法数据保留只读和 idle；越权、错窗口、缺卡片，以及关闭但仍有卡片的快照全部拒绝。
    func testSnapshotValidatesScopesAndLiveIdentity() throws {
        let raw = Data(#"{"version":1,"request_id":"read-a","result":{"status":"snapshot","grant":{"device_id":"phone-a","device_name":"iPhone","projects":[{"workspace_id":"project-a","access":"read"}],"revision":1,"revoked":false},"projects":[{"workspace_id":"project-a","display_name":"Norma","state":"open","route_id":"route-a","card_count":1,"cards":[{"node_id":1,"profile":"Codex","title":"修复登录","mobile_control":true,"state":"idle","run_id":null,"session_id":null,"task_title":null}]}]}}"#.utf8)
        func read(_ data: Data) throws -> MachineSnapshot {
            try MachineSnapshot(reply: WireReply.decode(data, expectedID: "read-a"), deviceID: "phone-a", workspaceID: "project-a")
        }
        let good = try read(raw)
        XCTAssertEqual(good.projects[0].cards?[0].state, "idle", "未派任务卡片不得显示运行")
        XCTAssertEqual(good.grant.projects[0].access, "read", "读取快照不能把权限升级")
        let text = String(decoding: raw, as: UTF8.self)
        XCTAssertThrowsError(try read(Data(text.replacingOccurrences(of: #""workspace_id":"project-a","display_name""#, with: #""workspace_id":"project-b","display_name""#).utf8)), "未授权 project-b 必须拒绝")
        XCTAssertThrowsError(try read(Data(text.replacingOccurrences(of: #""route_id":"route-a""#, with: #""route_id":null"#).utf8)), "打开的项目必须绑定实际窗口路由")
        XCTAssertThrowsError(try read(Data(text.replacingOccurrences(of: #""card_count":1"#, with: #""card_count":2"#).utf8)), "不能把缺一张卡的快照当完整列表")
        XCTAssertThrowsError(try read(Data(text.replacingOccurrences(of: #""state":"open""#, with: #""state":"closed""#).utf8)), "关闭项目不得携带仍然可操作的卡片快照")
    }

    /// 单项目快照应携带可供 iOS 创建卡片的项目 Agent 定义，并过滤不可创建项。
    ///
    /// 数据构造：4 个项目定义分别覆盖可用 Codex、已开卡 Claude、已禁用 Codex 和可用 Cursor。
    /// 执行过程：解码真实 snapshot 顶层 agents → 读取全部定义 → 计算可创建候选。
    /// 预期结果：完整保留 4 条服务端定义；候选包含启用、未开卡且使用 Codex/Cursor 的 Alan 与 Chloe。
    func testSnapshotExposesCreatableProjectAgentDefinitions() throws {
        let raw = Data(#"{"version":1,"request_id":"read-agents","result":{"status":"snapshot","grant":{"device_id":"phone-a","device_name":"iPhone","projects":[{"workspace_id":"project-a","access":"control"}],"revision":2,"revoked":false},"projects":[{"workspace_id":"project-a","display_name":"Norma","state":"open","route_id":"route-a","card_count":0,"cards":[]}],"agents":[{"definition_id":7,"name":"Alan","speech_alias":"艾伦","profile":"Codex","enabled":true,"active_node_id":null,"busy":false},{"definition_id":8,"name":"Bella","speech_alias":"贝拉","profile":"Claude","enabled":true,"active_node_id":9,"busy":false},{"definition_id":9,"name":"Bruno","speech_alias":"布鲁诺","profile":"Codex","enabled":false,"active_node_id":null,"busy":false},{"definition_id":10,"name":"Chloe","speech_alias":"克洛伊","profile":"Cursor","enabled":true,"active_node_id":null,"busy":false}]}}"#.utf8)

        let snapshot = try MachineSnapshot(
            reply: WireReply.decode(raw, expectedID: "read-agents"),
            deviceID: "phone-a",
            workspaceID: "project-a"
        )

        XCTAssertEqual(snapshot.agentDefinitions.count, 4, "单项目快照必须保留服务端返回的全部 Agent 定义")
        XCTAssertEqual(snapshot.creatableAgentDefinitions.map(\.name), ["Alan", "Chloe"], "候选必须保留可用 Cursor，同时排除已开卡和禁用定义")
        XCTAssertEqual(snapshot.creatableAgentDefinitions.first?.speechAlias, "艾伦", "候选应保留项目定义的语音别名用于解释身份")
        XCTAssertFalse(snapshot.creatableAgentDefinitions.contains(where: { $0.name == "Bella" }), "已有活动卡片的定义不能再次创建")
        XCTAssertTrue(snapshot.creatableAgentDefinitions.contains(where: { $0.profile == "Cursor" }), "iOS 必须把可用 Cursor 定义暴露给创建界面")
    }
}

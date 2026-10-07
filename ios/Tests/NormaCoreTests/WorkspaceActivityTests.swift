import XCTest
import Testing
@testable import NormaCore

/// 自动登记无需用户选空间，项目身份仍保留所属空间。
/// 数据构造：64位hex启动token，设备级登记省略desktop_workspace_id；两个区域同名但不同空间。
/// 执行过程：解码并校验自动登记→解码两个项目→校验它们不是同一个深链目标。
/// 预期结果：默认登记合法；跨空间的同名区域不被误合并。
@Test func automaticActivityRegistrationAndScopedProjects() throws {
    let raw=#"{"generation":"auto","environment":"sandbox","auto_start":true,"push_to_start_token":"TOKEN","dismissed":false}"#.replacingOccurrences(of:"TOKEN",with:String(repeating:"b",count:64))
    let registration=try JSONDecoder().decode(WorkspaceActivityRegistration.self,from:Data(raw.utf8))
    try registration.validate()
    let a=try JSONDecoder().decode(WorkspaceActivityContent.Project.self,from:Data(#"{"desktop_workspace_id":"space-a","workspace_name":"A","area_id":"area","name":"项目","status":"running","reason":"执行","agents":[],"other_agents":0}"#.utf8))
    let b=try JSONDecoder().decode(WorkspaceActivityContent.Project.self,from:Data(#"{"desktop_workspace_id":"space-b","workspace_name":"B","area_id":"area","name":"项目","status":"running","reason":"执行","agents":[],"other_agents":0}"#.utf8))
    #expect(a.id != b.id,"同名区域在不同工作空间必须有不同展示身份")
}

final class WorkspaceActivityTests: XCTestCase {
    /// 活动跳转与过期：链接只定位工作空间，不授权操作；120秒为新鲜度边界。
    /// 数据构造：mac + 64位证书hash；workspace/area各一个；100+120=220秒。
    /// 执行过程：合法链接往返→注入非法身份→读取到220秒→221秒过期。
    /// 预期结果：合法定位保留；错协议/非法区域被拒；过期必须显式展示。
    func testLinkAndFreshness() throws {
        let link = WorkspaceActivityLink(machineID: "mac-" + String(repeating:"a",count:64), workspaceID:"space",areaID:"area")
        XCTAssertEqual(WorkspaceActivityLink(url:link.url),link,"深链保留准确Mac与区域")
        XCTAssertNil(WorkspaceActivityLink(url:URL(string:"norma://activity?machine=x&workspace=space&area=area")!),"不接受非法Mac")
        XCTAssertNil(WorkspaceActivityLink(url:URL(string:"https://activity?machine=x")!),"网页不能冒充本地深链")
        let state=try JSONDecoder().decode(WorkspaceActivityContent.self,from:Data(#"{"name":"空间","updated_at":100,"active":true,"running_count":1,"approval_count":0,"review_count":0,"failure_count":0,"hidden_projects":0,"projects":[]}"#.utf8))
        XCTAssertFalse(state.isStale(at:Date(timeIntervalSince1970:219)),"两分钟内有效")
        XCTAssertTrue(state.isStale(at:Date(timeIntervalSince1970:220)),"两分钟达到边界应过期")
    }
    /// 本地生命周期与编码：首启或结束后已观察空闲才允许前台启动，8小时连续工作不得循环。
    /// 数据构造：start=100、end=100+8×3600；三种journal，quiet_seen布尔控制新周期。
    /// 执行过程：解码未启动/已结束忙碌/已结束空闲状态→验证foreground资格→校验有界Agent信息。
    /// 预期结果：初始与新工作可启动，忙碌旧周期不可启动；超长Agent动作拒绝。
    func testForegroundCycleAndAgentBounds() throws {
        func journal(_ text:String) throws -> WorkspaceActivitySubscription.Journal {try JSONDecoder().decode(WorkspaceActivitySubscription.Journal.self,from:Data(text.utf8))}
        XCTAssertTrue(try journal(#"{"quiet_seen":false}"#).canStartForeground,"首次工作可前台启动")
        XCTAssertFalse(try journal(#"{"started_at":100,"ended_at":28900,"quiet_seen":false}"#).canStartForeground,"连续八小时不能循环启动")
        XCTAssertTrue(try journal(#"{"started_at":100,"ended_at":28900,"quiet_seen":true}"#).canStartForeground,"空闲后的新工作可以启动")
        let invalid=#"{"name":"空间","updated_at":100,"active":true,"running_count":1,"approval_count":0,"review_count":0,"failure_count":0,"hidden_projects":0,"projects":[{"area_id":"area","name":"项目","status":"running","reason":"执行","other_agents":0,"agents":[{"name":"Agent","action":"ACTION"}]}]}"#.replacingOccurrences(of:"ACTION",with:String(repeating:"x",count:61))
        let content=try JSONDecoder().decode(WorkspaceActivityContent.self,from:Data(invalid.utf8))
        XCTAssertThrowsError(try content.validate(),"超过60字节的动作不能越过展示边界")
    }
    /// 未决审批跨重启保留：只有原目标和原决策的回执能消项。
    /// 数据构造：1个Mac/area/node=1，唯一op，allow决策；临时原子文件，隔离真实手机记录。
    /// 执行过程：保存→新store读取→核验原回执→注入其他节点和相反决策→移除成功项。
    /// 预期结果：重启不丢记录；错节点/决策不能报成功，真实成功后记录消失。
    func testPendingDecisionDurabilityAndReceiptIdentity() async throws {
        let dir=FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer {try? FileManager.default.removeItem(at:dir)}
        let target=try JSONDecoder().decode(WorkspaceActivityTarget.self,from:Data(#"{"area_id":"area","node_id":1,"profile":"Codex","run_id":"run","session_id":"session","turn_id":"turn","request_id":"request"}"#.utf8))
        var request=WorkspaceActivityRequest(kind:"decide",workspaceID:"space");request.target=target;request.operationID="op";request.decision = .allow
        let original=WorkspaceActivityDecisionStore.Pending(machineID:"mac-"+String(repeating:"a",count:64),request:request)
        let url=dir.appendingPathComponent("pending.json")
        try await WorkspaceActivityDecisionStore(url:url).save(original)
        let reopened=WorkspaceActivityDecisionStore(url:url)
        let pending=try await reopened.load()
        XCTAssertEqual(pending.count,1,"未决操作应跨store恢复")
        func receipt(node:Int,decision:String) throws -> MobileCommandReceipt {
            try JSONDecoder().decode(MobileCommandReceipt.self,from:Data("{\"device_id\":\"phone\",\"operation_id\":\"op\",\"workspace_id\":\"desktop:space:area\",\"route_id\":\"route\",\"state\":\"succeeded\",\"result\":{\"node_id\":\(node),\"run_id\":\"run\",\"session_id\":\"session\",\"turn_id\":\"turn\",\"request_id\":\"request\",\"decision\":\"\(decision)\"}}".utf8))
        }
        XCTAssertNoThrow(try original.validateReceipt(receipt(node:1,decision:"allow")),"原操作原目标应通过")
        XCTAssertThrowsError(try original.validateReceipt(receipt(node:2,decision:"allow")),"另一节点成功不能消项")
        XCTAssertThrowsError(try original.validateReceipt(receipt(node:1,decision:"deny")),"相反决策不能消项")
        try await reopened.remove("op")
        let empty=try await reopened.load();XCTAssertTrue(empty.isEmpty,"确认后的原操作应消项")
    }
}

private actor ActivityTransportProbe {var writes=0;func sent() {writes+=1}}
private struct ActivityTransport: WorkspaceChatAPI {
    let probe:ActivityTransportProbe
    func catalog() async throws -> WorkspaceCatalog {
        try JSONDecoder().decode(WorkspaceCatalog.self,from:Data(#"{"mac_id":"mac","workspaces":[],"available_projects":[]}"#.utf8))
    }
    func activity(_ request:WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse {await probe.sent();throw MobileError.timedOut}
    func receipt(for message:WorkspaceChatMessage) async throws -> WorkspaceChatReceipt? {nil}
    func submit(_ message:WorkspaceChatMessage) async throws -> WorkspaceChatReceipt {throw MobileError.unavailable}
    func events(workspaceID:String,after:UInt64) async throws -> [WorkspaceChatEvent] {[]}
    func configure(workspaceID:String,revision:UInt64,change:WorkspaceConfigurationChange) async throws {}
}
extension WorkspaceActivityTests {
    /// 写入响应丢失不重放：连接选择只读完成后，一次未知写入不得换连接再写。
    /// 数据构造：直连和中继分别计数；一个set_subscription请求，期望revision=0。
    /// 执行过程：直连catalog成功→写入一次丢回复→捕获超时→读取两条传输计数。
    /// 预期结果：直连恰为1，中继为0，必须保留未知结果错误。
    func testUncertainActivityMutationNeverReplays() async throws {
        let direct=ActivityTransportProbe();let relay=ActivityTransportProbe()
        let client=WorkspaceChatClient(direct:ActivityTransport(probe:direct),relay:{ActivityTransport(probe:relay)})
        var request=WorkspaceActivityRequest(kind:"set_subscription");request.expectedRevision=0
        do {_=try await client.activity(request);XCTFail("未知写入不能报成功")}
        catch {XCTAssertEqual(error as? MobileError,.timedOut,"应保留原未知写入错误")}
        let sent=await direct.writes;let replayed=await relay.writes
        XCTAssertEqual(sent,1,"直连写入仅提交一次")
        XCTAssertEqual(replayed,0,"未知写入不得在中继重放")
    }
}

/// 设备级读取经过实际客户端路由，决策仍必须指定工作空间。
/// 数据构造：summary/subscription各独立运行；直连与中继模拟超时并记录调用次数。
/// 执行过程：无空间detail在本地拒绝→默认读取到直连→超时切到中继一次。
/// 预期结果：detail不发送；读取到达两个传输边界，最终保留超时而非非法scope错误。
@Test(arguments:["summary","subscription"])
func automaticActivityRequestsReachTransport(kind:String) async throws {
    let direct=ActivityTransportProbe(),relay=ActivityTransportProbe()
    let client=WorkspaceChatClient(direct:ActivityTransport(probe:direct),relay:{ActivityTransport(probe:relay)})
    do {_=try await client.activity(.init(kind:"detail"));Issue.record("没有工作空间的详情应拒绝")}
    catch {#expect(error as? MobileError == .invalidResponse,"详情必须明确定位工作空间")}
    #expect(await direct.writes == 0,"非法详情不能到达直连")
    #expect(await relay.writes == 0,"非法详情不能到达中继")
    do {_=try await client.activity(.init(kind:kind));Issue.record("模拟超时不应返回成功")}
    catch {#expect(error as? MobileError == .timedOut,"默认总览读取必须进入传输层")}
    #expect(await direct.writes == 1,"默认读取应到达直连一次")
    #expect(await relay.writes == 1,"只读超时可切中继一次")
}

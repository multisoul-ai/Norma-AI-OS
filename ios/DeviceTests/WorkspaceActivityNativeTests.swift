import XCTest
import ActivityKit
import NormaCore
@testable import Norma

final class WorkspaceActivityNativeTests: XCTestCase {
    /// 默认登记与丢响应恢复：两台已配对Mac都自动登记，无需跟踪开关或工作空间选择。
    /// 数据构造：隔离UserDefaults；两个Mac各revision从0到1；set响应模拟超时。
    /// 执行过程：同步已配对列表→各写一次→丢响应后只读回查→重启读取原generation。
    /// 预期结果：两台Mac均恢复登记、scope为空；每台仅写一次，不能重复或串用generation。
    @MainActor func testLostRegistrationReplyRecoversByRead() async throws {
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {throw XCTSkip("此设备未开启实时活动")}
        let suite="activity-test-"+UUID().uuidString
        let defaults=try XCTUnwrap(UserDefaults(suiteName:suite));defer {defaults.removePersistentDomain(forName:suite)}
        let a="mac-"+String(repeating:"a",count:64),b="mac-"+String(repeating:"b",count:64)
        let source=ActivitySubscriptionProbe(),other=ActivitySubscriptionProbe()
        let manager=WorkspaceActivityManager(defaults:defaults,send:{machine,request in try await (machine==a ? source:other).request(request)})
        await manager.synchronize(foreground:false,force:true,machines:[a,b])
        XCTAssertNotNil(manager.registrations[a],"Mac A必须自动登记")
        XCTAssertNotNil(manager.registrations[b],"当前页面未选择的Mac B也必须自动登记")
        XCTAssertNil(manager.registrations[a]?.workspaceID,"自动登记不能绑定手动选择的空间")
        XCTAssertNil(manager.error,"匹配的回查结果应解除未知响应错误")
        let writes=await source.writes
        XCTAssertEqual(writes,1,"响应丢失后不能重复写入")
        let otherWrites=await other.writes
        XCTAssertEqual(otherWrites,1,"另一台Mac同样只登记一次")
        let restored=WorkspaceActivityManager(defaults:defaults)
        XCTAssertEqual(restored.registrations[a]?.generation,manager.registrations[a]?.generation,"重启必须保留A原generation")
        XCTAssertEqual(restored.registrations[b]?.generation,manager.registrations[b]?.generation,"重启必须保留B原generation")
        XCTAssertNotEqual(manager.registrations[a]?.generation,manager.registrations[b]?.generation,"两台Mac不能串用generation")
    }
    /// 忘记电脑与自动登记交错：清理必须排在迟到的写入之后，不能重新开启旧Mac。
    /// 数据构造：一个Mac；第一次登记在服务端写入后暂停回复；清理闭包记录调用次数。
    /// 执行过程：开始同步→写入暂停→忘记Mac→释放旧回复→等待清理完成→再次同步旧列表。
    /// 预期结果：立即移除本地登记；迟到写入后仅清理一次；旧列表不再登记。
    @MainActor func testForgetWaitsForInflightEnrollment() async throws {
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {throw XCTSkip("此设备未开启实时活动")}
        let suite="activity-forget-"+UUID().uuidString
        let defaults=try XCTUnwrap(UserDefaults(suiteName:suite));defer {defaults.removePersistentDomain(forName:suite)}
        let mac="mac-"+String(repeating:"a",count:64),probe=ActivityRemovalProbe()
        let manager=WorkspaceActivityManager(defaults:defaults,send:{_,request in try await probe.request(request)})
        let sync=Task {await manager.synchronize(foreground:false,force:true,machines:[mac])}
        // 5秒/50ms=100次让出主线程，等待实际写入边界而非固定sleep。
        let deadline=ContinuousClock.now.advanced(by:.seconds(5))
        while await probe.writes==0 && ContinuousClock.now<deadline {try await Task.sleep(for:.milliseconds(50))}
        let writes=await probe.writes;XCTAssertEqual(writes,1,"测试必须到达未完成的登记写入")
        await manager.forget(mac,cleanup:{await probe.remove()})
        XCTAssertNil(manager.registrations[mac],"忘记时必须立即清除本地意图")
        let early=await probe.removals;XCTAssertEqual(early,0,"清理不能抢在迟到登记之前")
        await probe.release();await sync.value
        // 相同5秒预算等待清理闭包执行完毕，50ms一次让出。
        let cleanupDeadline=ContinuousClock.now.advanced(by:.seconds(5))
        while await probe.removals==0 && ContinuousClock.now<cleanupDeadline {try await Task.sleep(for:.milliseconds(50))}
        await manager.synchronize(foreground:false,force:true,machines:[mac])
        let removed=await probe.removals,finalWrites=await probe.writes
        XCTAssertEqual(removed,1,"迟到登记后必须执行一次清理")
        XCTAssertEqual(finalWrites,1,"忘记期间的旧配对列表不能再次登记")
        XCTAssertNil(manager.registrations[mac],"迟到回复不得恢复本地登记")
    }
    /// 原生扩展协议：APNs attributes/content解码与设备上的ActivityKit类型一致，扩展随App安装。
    /// 数据构造：1个工作空间、3项目、每项目1个具名Agent；updated=100，stale=100+120。
    /// 执行过程：解码服务端字段→属性往返→创建ActivityContent→检查嵌入的Widget扩展。
    /// 预期结果：真实类型保留周期和三个项目；错字段不可静默替代；扩展bundle必须存在。
    func testNativeAttributesAndEmbeddedExtension() throws {
        let attributes=try JSONDecoder().decode(WorkspaceActivityAttributes.self,from:Data(#"{"machine_id":"mac-test","desktop_workspace_id":"space","generation":"generation","cycle_id":"cycle","started_at":100}"#.utf8))
        XCTAssertEqual(attributes.workspaceID,"space","服务端工作空间键必须一致")
        let encoded=try JSONSerialization.jsonObject(with:JSONEncoder().encode(attributes)) as? [String:Any]
        XCTAssertEqual(encoded?["cycle_id"] as? String,"cycle","属性往返保留周期")
        XCTAssertNil(encoded?["workspaceID"],"不能把Swift属性名发给APNs")
        let content=try JSONDecoder().decode(WorkspaceActivityContent.self,from:Data(Self.content.utf8))
        try content.validate()
        let activity=ActivityContent(state:content,staleDate:Date(timeIntervalSince1970:220))
        XCTAssertEqual(activity.state.projects.count,3,"实际ActivityKit状态应承载三个项目")
        XCTAssertFalse(content.isStale(at:Date(timeIntervalSince1970:219)),"新鲜度边界前不能过期")
        XCTAssertTrue(content.isStale(at:activity.staleDate!),"staleDate必须与两分钟边界相符")
        let plugins=try XCTUnwrap(Bundle.main.builtInPlugInsURL,"应用必须嵌入Widget扩展")
        XCTAssertTrue(FileManager.default.fileExists(atPath:plugins.appendingPathComponent("NormaActivity.appex/Info.plist").path),"Widget扩展应实际安装")
    }
    /// 原生本地活动生命周期：前台启动、过期更新、恢复与结束都走ActivityKit。
    /// 数据构造：1个合成工作空间，无APNs token，无真实审批；staleDate=现在-1秒。
    /// 执行过程：系统允许时request→更新为过期→恢复新鲜→end→核对系统活动状态。
    /// 预期结果：本地活动身份保持，更新状态真实恢复，结束后不能再作为活跃周期。
    @MainActor func testNativeLocalActivityLifecycle() async throws {
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {throw XCTSkip("当前设备系统未允许实时活动")}
        let attributes=WorkspaceActivityAttributes(machineID:"mac-test",workspaceID:"native-test",generation:"native-test",cycleID:UUID().uuidString,startedAt:UInt64(Date.now.timeIntervalSince1970))
        let content=try JSONDecoder().decode(WorkspaceActivityContent.self,from:Data(Self.content.utf8))
        let activity=try Activity.request(attributes:attributes,content:ActivityContent(state:content,staleDate:.now.addingTimeInterval(-1)),pushType:nil)
        XCTAssertEqual(activity.attributes.workspaceID,"native-test","必须创建合成目标的本地活动")
        var recovered=false
        let updates=Task {for await value in activity.contentUpdates {if value.staleDate.map({$0>Date.now}) ?? false {recovered=true;break}}}
        defer {updates.cancel()}
        await activity.update(ActivityContent(state:content,staleDate:.now.addingTimeInterval(120)))
        // 系统状态通过异步序列回送：最多5秒，每50ms让出主线程（5/0.05=100次）。
        let updateDeadline=ContinuousClock.now.advanced(by:.seconds(5))
        while !recovered && ContinuousClock.now<updateDeadline {try await Task.sleep(for:.milliseconds(50))}
        XCTAssertTrue(recovered,"恢复更新必须由真实ActivityKit事件确认")
        var ended=false
        let states=Task {for await value in activity.activityStateUpdates {if value == .ended || value == .dismissed {ended=true;break}}}
        defer {states.cancel()}
        await activity.end(nil,dismissalPolicy:.immediate)
        // 同一系统回送预算：5秒，50ms异步让出，不阻塞UI线程。
        let endDeadline=ContinuousClock.now.advanced(by:.seconds(5))
        while !ended && ContinuousClock.now<endDeadline {try await Task.sleep(for:.milliseconds(50))}
        XCTAssertTrue(ended,"结束必须由真实ActivityKit状态事件确认")
        XCTAssertFalse(Activity<WorkspaceActivityAttributes>.activities.contains(where:{$0.id==activity.id && $0.activityState == .active}),"结束后不能继续作为活跃活动")
    }
    /// Computer Use 专用本地样本：显式启用时留下真实系统卡片，普通测试不保留活动。
    /// 数据构造：3项目合成内容，无网络和审批权限；fresh/stale/cleanup三个显式测试模式。
    /// 执行过程：清理本测试generation→按模式创建本地Activity→由Computer Use检查锁屏；cleanup结束样本。
    /// 预期结果：系统拥有真实Widget卡片，普通测试跳过；样本只能指向不存在的测试Mac。
    @MainActor func testLockScreenVisualFixture() async throws {
        let mode=ProcessInfo.processInfo.environment["NORMA_ACTIVITY_VISUAL"] ?? ""
        guard ["fresh","stale","cleanup"].contains(mode) else {throw XCTSkip("仅在显式Computer Use验收时创建样本")}
        for activity in Activity<WorkspaceActivityAttributes>.activities where activity.attributes.generation=="visual-test" {await activity.end(nil,dismissalPolicy:.immediate)}
        if mode=="cleanup" {return}
        let attributes=WorkspaceActivityAttributes(machineID:"mac-"+String(repeating:"f",count:64),generation:"visual-test",cycleID:UUID().uuidString,startedAt:UInt64(Date.now.timeIntervalSince1970))
        let source=Self.content.replacingOccurrences(of:"\"updated_at\":100",with:"\"updated_at\":\(UInt64(Date.now.timeIntervalSince1970))")
        let content=try JSONDecoder().decode(WorkspaceActivityContent.self,from:Data(source.utf8))
        try content.validate()
        let activity=try Activity.request(attributes:attributes,content:ActivityContent(state:content,staleDate:.now.addingTimeInterval(mode=="stale" ? -1:120)),pushType:nil)
        XCTAssertEqual(activity.attributes.generation,"visual-test","只有显式样本可保留供真实界面验收")
    }
    private static let content = #"{"name":"Mac 工作总览","updated_at":100,"active":true,"running_count":1,"approval_count":1,"review_count":1,"failure_count":0,"hidden_projects":1,"projects":[{"desktop_workspace_id":"work","workspace_name":"产品研发","area_id":"approval","name":"支付系统","status":"approval","reason":"Maya · 等待确认 Bash","agents":[{"name":"Maya","action":"工具调用待确认"}],"other_agents":0},{"desktop_workspace_id":"marketing","workspace_name":"品牌增长","area_id":"review","name":"官网","status":"review","reason":"Leo · 2个文件待验收","agents":[{"name":"Leo","action":"成果已冻结"}],"other_agents":0},{"desktop_workspace_id":"work","workspace_name":"产品研发","area_id":"running","name":"移动端","status":"running","reason":"Kai · 正在实现登录","agents":[{"name":"Kai","action":"实现登录"}],"other_agents":0}]}"#
}
private actor ActivityRemovalProbe {
    var writes=0;var removals=0
    private var releaseReply:CheckedContinuation<Void,Never>?
    private var registration:WorkspaceActivityRegistration?
    func request(_ request:WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse {
        if request.kind=="set_subscription" {
            writes+=1;registration=request.registration
            await withCheckedContinuation {releaseReply=$0}
        }
        let value:Any=try registration.map {try JSONSerialization.jsonObject(with:JSONEncoder().encode($0))} ?? NSNull()
        return try JSONDecoder().decode(WorkspaceActivityResponse.self,from:JSONSerialization.data(withJSONObject:["subscription":["revision":writes,"registration":value,"journal":["quiet_seen":false]]]))
    }
    func release() {releaseReply?.resume();releaseReply=nil}
    func remove() {removals+=1;registration=nil}
}
private actor ActivitySubscriptionProbe {
    var writes=0
    private var revision:UInt64=0
    private var registration:WorkspaceActivityRegistration?
    func request(_ request:WorkspaceActivityRequest) throws -> WorkspaceActivityResponse {
        if request.kind=="set_subscription" {writes+=1;revision+=1;registration=request.registration;throw MobileError.timedOut}
        let value:Any=try registration.map {try JSONSerialization.jsonObject(with:JSONEncoder().encode($0))} ?? NSNull()
        let data=try JSONSerialization.data(withJSONObject:["subscription":["revision":revision,"registration":value,"journal":["quiet_seen":false]]])
        return try JSONDecoder().decode(WorkspaceActivityResponse.self,from:data)
    }
}

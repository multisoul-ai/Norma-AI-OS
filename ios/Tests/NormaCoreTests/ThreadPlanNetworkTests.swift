import XCTest
@testable import NormaCore

@MainActor final class ThreadPlanNetworkTests: XCTestCase {
    /// 正式Swift客户端通过本地正式网关获取候选，并交给既有Mac任务队列。
    /// 数据构造：1台受邀手机、同一Mac的2项目/2位同名Alice；模型3轮=执行+讨论+截断。
    /// 执行过程：兑换邀请→精确派工并重开→同回合重放→只讨论→截断拒绝→撤权。
    /// 预期结果：仅iOS Alice有1项待发任务；讨论无新任务，半份回复不落盘，撤权不请求模型。
    func testPlanThroughLocalGateway() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_THREAD_PLAN_FIXTURE"] else {
            throw XCTSkip("由 check-thread-plan-native.mjs 启动正式本地HTTP与测试供应商")
        }
        struct Fixture: Decodable { let port: UInt16; let admin: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let client = try CloudTextClient(localPort: fixture.port)
        let origin = URL(string: "http://127.0.0.1:\(fixture.port)/")!
        let session = URLSession(configuration: .ephemeral)
        defer { session.invalidateAndCancel() }
        func admin(_ route: String, body: [String: String] = [:]) async throws -> Data {
            var request = URLRequest(url: origin.appending(path: "v1/admin/devices/" + route))
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.setValue("Bearer \(fixture.admin)", forHTTPHeaderField: "Authorization")
            request.httpBody = try JSONEncoder().encode(body)
            let (data, response) = try await session.data(for: request)
            XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200, "本地隔离准入操作应成功")
            return data
        }
        struct Invitation: Decodable { let code: String }
        let invite = try JSONDecoder().decode(Invitation.self, from: await admin("invite"))
        let credential = try await client.claim(code: invite.code, name: "编排网络验收")
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = directory.appendingPathComponent("workspace.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: database)
        var workspace = try await store.create(name: "两个项目")
        // SHA-256指纹：32字节×2位hex = 64位，不放宽正式Mac身份格式。
        let mac = "mac-" + String(repeating: "a", count: 64)
        workspace.projects = [.init(machineID: mac, projectID: "ios", name: "iOS", machineName: "Mac"),
                              .init(machineID: mac, projectID: "api", name: "API", machineName: "Mac")]
        workspace.agents = [.init(machineID: mac, projectID: "ios", definitionID: 2, name: "Alice", profile: "Codex"),
                            .init(machineID: mac, projectID: "api", definitionID: 3, name: "Alice", profile: "Codex")]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let prepared = try await store.prepareNormaTurn(threadID: thread.id, messageID: UUID().uuidString, text: "@Norma 请检查iOS首屏")
        let response = try await client.planThread(prepared.request, token: credential.token)
        XCTAssertEqual(response.assignments.first?.targetID, workspace.agents[0].id, "同名候选须精确路由到iOS Alice")
        XCTAssertNotEqual(response.assignments.first?.targetID, workspace.agents[1].id, "不能误派给API Alice")
        _ = try await store.applyNormaTurn(response, prepared: prepared)
        let reopened = CollaborationWorkspaceStore(databaseURL: database)
        let tasks = try await reopened.threadTasks(threadID: thread.id)
        XCTAssertEqual(tasks.count, 1, "真实HTTP结果应进入既有待发队列")
        XCTAssertNil(tasks[0].receipt, "模型候选不能冒充Mac已收到")
        do {
            _ = try await client.planThread(prepared.request, token: credential.token)
            XCTFail("相同回合不应再次调用模型")
        } catch CloudTextError.duplicateTurn { }
        let discussion = try await store.prepareNormaTurn(threadID: thread.id, messageID: UUID().uuidString, text: "@Norma 先讨论，不开工")
        let answer = try await client.planThread(discussion.request, token: credential.token)
        XCTAssertTrue(answer.assignments.isEmpty, "讨论应只有回复")
        _ = try await store.applyNormaTurn(answer, prepared: discussion)
        let unchanged = try await reopened.threadTasks(threadID: thread.id)
        XCTAssertEqual(unchanged, tasks, "讨论不能创建或改写任务")
        let draft = WorkThreadDraft(text: "@Norma 截断测试")
        try await store.saveMessageDraft(draft, threadID: thread.id)
        let incomplete = try await store.prepareNormaTurn(threadID: thread.id, messageID: UUID().uuidString,
            text: draft.text, clearingDraftRevision: draft.revision)
        do {
            _ = try await client.planThread(incomplete.request, token: credential.token)
            XCTFail("服务端拒绝截断候选，手机不能把它当成功")
        } catch CloudTextError.invalidResponse { }
        let kept = try await reopened.messageDrafts()[thread.id]
        let history = try await reopened.messages(threadID: thread.id)
        XCTAssertEqual(kept, draft, "请求失败必须保留原草稿")
        XCTAssertEqual(history.count, 4, "只有执行和讨论两轮的完整消息应落盘")
        XCTAssertFalse(history.contains { $0.id == incomplete.request.turnID }, "不能保存半份Norma回复")
        _ = try await admin("revoke", body: ["device_id": credential.deviceID])
        do {
            _ = try await client.planThread(incomplete.request, token: credential.token)
            XCTFail("撤权设备不得继续请求编排")
        } catch CloudTextError.revoked { }
    }
}

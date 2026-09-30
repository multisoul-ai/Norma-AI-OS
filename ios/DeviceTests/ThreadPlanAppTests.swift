import XCTest
@testable import NormaCore

private struct PlanUIFixture: Decodable {
    let port: UInt16
    let admin: String
    let namespace: String
    var directory: URL { URL.applicationSupportDirectory.appendingPathComponent("Norma/UITestWorkspaces/\(namespace)") }
    var service: String { "com.norma-ai.os.ios.cloud.local.\(port).\(namespace)" }
    static func load() throws -> Self {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "thread-plan-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_THREAD_PLAN_FIXTURE"],
              FileManager.default.fileExists(atPath: path) else { throw XCTSkip("需要本机编排UI服务") }
        let result = try JSONDecoder().decode(Self.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        guard result.port > 0, UUID(uuidString: result.namespace) != nil else { throw CloudTextError.invalidRequest }
        return result
    }
    func adminRequest(_ route: String, body: [String: String] = [:]) async throws -> Data {
        let session = URLSession(configuration: .ephemeral)
        defer { session.invalidateAndCancel() }
        var request = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/v1/admin/devices/\(route)")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(admin)", forHTTPHeaderField: "Authorization")
        request.httpBody = try JSONEncoder().encode(body)
        let (data, response) = try await session.data(for: request)
        guard (response as? HTTPURLResponse)?.statusCode == 200 else { throw CloudTextError.invalidResponse }
        return data
    }
}

final class ThreadPlanAppTests: XCTestCase {
    /// 准备本地云准入和空间配置，不预填消息、回复或任务。
    /// 数据构造：1隔离命名空间、1测试Mac引用、1项目/Agent；Mac未配对，任务只能待发送。
    /// 执行过程：拒绝已有测试库/凭据→真实HTTP兑换邀请→保存隔离云授权→创建项目成员配置。
    /// 预期结果：待发队列为空，UI须真实输入并请求网关后才产生候选；不访问正式模型或用户项目。
    func testPreparePlanUI() async throws {
        let fixture = try PlanUIFixture.load()
        let vault = CloudCredentialVault(service: fixture.service)
        let previous = try await vault.load()
        guard previous == nil, !FileManager.default.fileExists(atPath: fixture.directory.path) else {
            XCTFail("不能覆盖已有云授权或测试数据"); return
        }
        struct Invitation: Decodable { let code: String }
        let invitation = try JSONDecoder().decode(Invitation.self, from: await fixture.adminRequest("invite"))
        let client = try CloudTextClient(localPort: fixture.port)
        let credential = try await client.claim(code: invitation.code, name: "原生编排验收")
        try await vault.save(credential)
        let store = CollaborationWorkspaceStore(databaseURL: fixture.directory.appendingPathComponent("workspaces.sqlite"))
        var workspace = try await store.create(name: "移动端体验（测试）")
        // 测试身份长度遵循SHA-256：32字节×2位hex = 64；不登记成实际电脑。
        let mac = "mac-" + String(repeating: "e", count: 64)
        workspace.projects = [.init(machineID: mac, projectID: "ios", name: "iOS", machineName: "测试 Mac（未连接）")]
        workspace.agents = [.init(machineID: mac, projectID: "ios", definitionID: 2, name: "Alice", profile: "Codex")]
        try await store.save(workspace)
        let tasks = try await store.pendingThreadTasks()
        let threads = try await store.threads(workspaceID: workspace.id)
        XCTAssertTrue(tasks.isEmpty, "准备阶段不得伪造派工")
        XCTAssertTrue(threads.isEmpty, "线程也应由UI创建")
    }

    /// 结束验收后撤销并删除仅本轮云准入，保留隔离消息库作为证据。
    /// 数据构造：服务端随机设备ID及命名空间Keychain服务，均只属于本轮fixture。
    /// 执行过程：读取精确凭据→本地服务撤销→删除这条Keychain记录→再次读取。
    /// 预期结果：临时凭据不存在，其他设备/默认云授权完全不受影响。
    func testCleanupPlanUI() async throws {
        let fixture = try PlanUIFixture.load()
        let vault = CloudCredentialVault(service: fixture.service)
        if let credential = try await vault.load() {
            _ = try await fixture.adminRequest("revoke", body: ["device_id": credential.deviceID])
            try await vault.remove()
        }
        let remaining = try await vault.load()
        XCTAssertNil(remaining, "验收结束不能残留临时云授权")
    }
}

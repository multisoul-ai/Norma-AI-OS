import XCTest
@testable import Norma
@testable import NormaCore

final class AgentCreationLocalAppTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Cloud: Decodable {
            let deviceID: String; let token: String
            enum CodingKeys: String, CodingKey { case deviceID = "device_id", token }
        }
        let machineID: String; let workspaceID: String; let areaID: String; let otherWorkspaceID: String
        let certificateSHA256: String; let endpoints: [URL]; let cloud: Cloud; let relayPort: UInt16
        let restartRequest: WorkspaceAgentCreateRequest?
    }
    /// 场景：App Intent 在聊天页未打开时创建真实Agent，两次独立运行各创建一次。
    /// 数据构造：隔离Mac已配对、1个真实项目；Codex；名称由Mac分配；没有任务文本。
    /// 执行过程：读缓存选目标 → 连续运行两次Intent → 读真实目录 → 查询另一个空间。
    /// 预期结果：两张不同真实卡片加入精确区域；不是已有卡片；其它空间没有卡片；没有任务派发。
    @MainActor func testIntentCreatesTwoRealCardsWithoutChatPageOrTasks() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要隔离本地 Mac fixture") }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let environment = AgentCreationEnvironment.shared
        let client = try await environment.client(macID: fixture.machineID)
        let before = try await client.catalog()
        try await environment.store.saveCatalog(before)
        let workspace = try XCTUnwrap(before.workspaces.first { $0.id == fixture.workspaceID })
        let project = try XCTUnwrap(workspace.projects.first { $0.areaID == fixture.areaID })
        let oldIDs = Set(workspace.agents.map { $0.agent.card.nodeID })
        var intent = CreateNormaAgentIntent()
        intent.workspace = NormaWorkspaceEntity(macID: fixture.machineID, workspaceID: workspace.id, name: workspace.name, macName: "验收Mac")
        intent.project = NormaProjectEntity(macID: fixture.machineID, workspaceID: workspace.id, areaID: project.areaID, name: project.name, location: project.root)
        intent.kind = .codex
        let first = try await intent.perform()
        let second = try await intent.perform()
        XCTAssertNotEqual(first.value, second.value, "每次独立运行应返回不同自动名称")
        let after = try await client.catalog()
        let created = try XCTUnwrap(after.workspaces.first { $0.id == fixture.workspaceID }).agents.filter { !oldIDs.contains($0.agent.card.nodeID) }
        XCTAssertEqual(created.count, 2, "两次Intent必须在Mac上实际创建两张卡片")
        XCTAssertEqual(created[0].project.areaID, fixture.areaID, "首张卡片必须进入指定区域")
        XCTAssertEqual(created[1].project.areaID, fixture.areaID, "第二张卡片也必须使用同一真实区域")
        XCTAssertNotEqual(created[0].agent.definitionID, created[1].agent.definitionID, "不能复用原Agent定义冒充新建")
        XCTAssertFalse(after.workspaces.first { $0.id == fixture.otherWorkspaceID }?.agents.contains { $0.agent.definitionID == created[0].agent.definitionID } == true, "不能路由到桌面当前其它空间")
        XCTAssertFalse(created[0].agent.card.isBusy, "创建本身不派发任务")
        XCTAssertNil(created[0].agent.card.taskTitle, "不能给新卡片附加任务")
        await client.invalidateRelay()
    }

    /// 场景：直连创建后换到本地加密中继重提原操作，只创建一张真实卡片。
    /// 数据构造：1个已配对的隔离Mac、1个项目与稳定operation；40次×500ms=20s覆盖桌面持久化预算。
    /// 执行过程：读直连目录 → 提交创建 → 中继重提同ID → 中继轮询 → 比较前后卡片和另一空间。
    /// 预期结果：相同operation与卡片身份；只新增1个定义；无任务；另一空间没有该卡。
    @MainActor func testRelayRetryCreatesOneRealCardWithoutTask() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要本地隔离 Mac 与中继") }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let machine = try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: [
            "id": fixture.machineID, "name": "创建验收 Mac", "certificateSHA256": fixture.certificateSHA256,
            "endpoints": fixture.endpoints.map(\.absoluteString)
        ]))
        let computers = ComputerModel()
        let direct = DirectWorkspaceChatAPI(machine: machine, client: computers.client)
        let connection = try await CollaborationRelayClient(localPort: fixture.relayPort).connect(machine,
            cloud: CloudDeviceCredential(deviceID: fixture.cloud.deviceID, token: fixture.cloud.token), vault: computers.vault)
        let relay = RelayWorkspaceChatAPI(connection: connection)
        let before = try await direct.catalog()
        let space = try XCTUnwrap(before.workspaces.first { $0.id == fixture.workspaceID })
        let originalIDs = Set(space.agents.map { $0.agent.definitionID })
        let request = WorkspaceAgentCreateRequest(workspaceID: space.id, areaID: fixture.areaID, profile: "Codex", revision: space.revision)
        let accepted = try await direct.createAgent(request)
        let retry = try await relay.createAgent(request)
        XCTAssertEqual(retry.operationID, accepted.operationID, "连接切换不能换成新操作")
        var receipt = retry
        for _ in 0..<40 where !receipt.completed {
            try await Task.sleep(for: .milliseconds(500))
            let queried = try await relay.creationReceipt(request)
            receipt = try XCTUnwrap(queried)
        }
        XCTAssertEqual(receipt.state, "succeeded", "本地中继应返回真实成功回执而非仅接收")
        let directReceipt = try await direct.creationReceipt(request)
        let original = try XCTUnwrap(directReceipt)
        XCTAssertEqual(original.result, receipt.result, "直连与中继必须返回同一实际定义和卡片")
        let after = try await relay.catalog()
        let added = try XCTUnwrap(after.workspaces.first { $0.id == fixture.workspaceID }).agents.filter { !originalIDs.contains($0.agent.definitionID) }
        XCTAssertEqual(added.count, 1, "同operation不能因连接切换重复新建")
        let member = try XCTUnwrap(added.first)
        XCTAssertEqual(member.project.areaID, fixture.areaID, "不能依据当前桌面选择推断项目")
        XCTAssertEqual(member.agent.definitionID, receipt.result?.definitionID, "目录定义应来自实际创建回执")
        XCTAssertNil(member.agent.card.taskTitle, "创建不应派发任务")
        XCTAssertFalse(member.agent.card.isBusy, "空闲卡片不能伪装成执行任务")
        XCTAssertFalse(after.workspaces.first { $0.id == fixture.otherWorkspaceID }?.agents.contains { $0.agent.definitionID == member.agent.definitionID } == true, "其它空间不能出现此定义的卡片")
        await relay.close()
    }

    /// 场景：真实 Mac 重启后查询并重提已成功操作，不再次创建 Agent。
    /// 数据构造：重启前 App 创建 Ethan 的固定操作；精确 workspace/area 和持久回执；没有新任务。
    /// 执行过程：读重启后的目录 → 查询原回执 → 重提原请求 → 再读目录。
    /// 预期结果：原定义与卡片仍在原区域；回执身份不变；成员数量不增加；其它空间没有该卡片。
    @MainActor func testRestartRetryKeepsOriginalCardWithoutCreatingAgain() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要已实际重启的隔离 Mac") }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let request = try XCTUnwrap(fixture.restartRequest, "必须使用重启前保存的原操作，不能生成新 ID")
        let client = try await AgentCreationEnvironment.shared.client(macID: fixture.machineID)
        let before = try await client.catalog()
        let originalReceipt = try await client.creationReceipt(request)
        let original = try XCTUnwrap(originalReceipt, "重启必须保留原创建账本")
        let result = try XCTUnwrap(original.result, "成功回执必须保存实际定义与卡片")
        XCTAssertEqual(original.state, "succeeded", "重启不能把已成功创建改为失败")
        XCTAssertEqual(original.operationID, "09D11E45-274F-41B5-BCDB-113DC3D07084", "必须核查实际 App 创建操作")
        XCTAssertEqual(result.definitionID, 7, "Ethan 原定义为 7，不能用新定义替代")
        XCTAssertEqual(result.card?.nodeID, 7, "Ethan 原卡片为 7，不能恢复到其它卡片")
        let retried = try await client.createAgent(request)
        XCTAssertEqual(retried.result, result, "重提原操作必须返回原回执身份")
        let after = try await client.catalog()
        let beforeSpace = try XCTUnwrap(before.workspaces.first { $0.id == fixture.workspaceID })
        let afterSpace = try XCTUnwrap(after.workspaces.first { $0.id == fixture.workspaceID })
        XCTAssertEqual(afterSpace.agents.count, beforeSpace.agents.count, "Mac 重启后的重试不得新增卡片")
        let member = try XCTUnwrap(afterSpace.agents.first { $0.agent.definitionID == 7 })
        XCTAssertEqual(member.agent.card.nodeID, 7, "目录必须继续指向原卡片")
        XCTAssertEqual(member.project.areaID, fixture.areaID, "恢复不能改变项目区域")
        XCTAssertNil(member.agent.card.taskTitle, "查询与重提创建均不能派发任务")
        XCTAssertFalse(after.workspaces.first { $0.id == fixture.otherWorkspaceID }?.agents.contains { $0.agent.definitionID == 7 } == true, "其它空间不能收到恢复卡片")
        await client.invalidateRelay()
    }
}

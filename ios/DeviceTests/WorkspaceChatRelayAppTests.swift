import XCTest
@testable import Norma
@testable import NormaCore

final class WorkspaceChatRelayAppTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Mac: Decodable { let deviceID: String; let token: String }
        struct Cloud: Decodable {
            let deviceID: String; let token: String
            enum CodingKeys: String, CodingKey { case deviceID = "device_id", token }
        }
        let machineID: String; let certificateSHA256: String; let endpoints: [URL]
        let mac: Mac; let cloud: Cloud; let relayPort: UInt16
        let workspaceID: String; let otherWorkspaceID: String
    }

    /// 场景：真实 TLS 与本地加密中继共用同一持久群聊入口，切换连接不会重复接收讨论。
    /// 数据构造：隔离 Mac 的已配对设备和本地 Gateway 正式签发凭据；两个不同身份的工作空间。
    /// 执行过程：直连目录 → 中继目录 → 直连发送讨论 → 中继再次提交相同操作 → 重连按游标恢复。
    /// 预期结果：两端目录身份相同；received 仅一条；另一个空间没有此操作；普通讨论不能产生 dispatch。
    @MainActor func testLocalRelaySharesReceiptAndReconnectsWithoutDuplicateDispatch() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"] else {
            throw XCTSkip("仅本地隔离 App 与中继验收，蜂窝网络由用户测试")
        }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let machine = try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: [
            "id": fixture.machineID, "name": "群聊本地验收 Mac", "certificateSHA256": fixture.certificateSHA256,
            "endpoints": fixture.endpoints.map(\.absoluteString)
        ]))
        let computers = ComputerModel()
        let direct = DirectWorkspaceChatAPI(machine: machine, client: computers.client)
        let connection = try await CollaborationRelayClient(localPort: fixture.relayPort).connect(machine,
            cloud: CloudDeviceCredential(deviceID: fixture.cloud.deviceID, token: fixture.cloud.token), vault: computers.vault)
        let relay = RelayWorkspaceChatAPI(connection: connection)
        let localCatalog = try await direct.catalog()
        let remoteCatalog = try await relay.catalog()
        XCTAssertEqual(remoteCatalog.macID, localCatalog.macID, "中继必须连接同一 Mac")
        XCTAssertEqual(remoteCatalog.workspaces.map(\.id), localCatalog.workspaces.map(\.id), "中继与直连不能生成两套空间")
        let message = WorkspaceChatMessage(workspaceID: fixture.otherWorkspaceID, text: "只讨论首屏布局，不执行任务。")
        let accepted = try await direct.submit(message)
        let repeated = try await relay.submit(message)
        XCTAssertEqual(repeated.operationID, accepted.operationID, "换连接必须保留原操作身份")
        let events = try await relay.events(workspaceID: message.workspaceID, after: 0).filter { $0.operationID == message.operationID }
        XCTAssertEqual(events.filter { $0.kind == "received" }.count, 1, "同一操作必须只接收一次")
        XCTAssertFalse(events.contains { $0.kind == "dispatch" || $0.kind == "submitted" }, "普通讨论不能派工")
        let other = try await relay.events(workspaceID: fixture.workspaceID, after: 0)
        XCTAssertFalse(other.contains { $0.operationID == message.operationID }, "另一工作空间不能读到此消息")
        await relay.close()
        let newConnection = try await CollaborationRelayClient(localPort: fixture.relayPort).connect(machine,
            cloud: CloudDeviceCredential(deviceID: fixture.cloud.deviceID, token: fixture.cloud.token), vault: computers.vault)
        let reconnected = RelayWorkspaceChatAPI(connection: newConnection)
        let recovered = try await reconnected.receipt(for: message)
        XCTAssertEqual(recovered?.operationID, message.operationID, "重连必须查询原回执")
        let cursor = events.last?.cursor ?? 0
        let incremental = try await reconnected.events(workspaceID: message.workspaceID, after: cursor)
        XCTAssertFalse(incremental.contains { $0.cursor <= cursor }, "游标补齐不能重传旧事件")
        await reconnected.close()
    }
}

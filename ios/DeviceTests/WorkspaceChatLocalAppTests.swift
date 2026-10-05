import XCTest
import Security
@testable import Norma
@testable import NormaCore

final class WorkspaceChatLocalAppTests: XCTestCase {
    /// 场景：模拟器通过真实 TLS Mac 读取共享目录；局域网群聊不要求手机云端模型授权。
    /// 数据构造：独立 Mac fixture 的随机设备凭据、固定证书 pin、两个空间及原生 Codex 卡片；云凭据为空。
    /// 执行过程：写入测试设备 Keychain → 保存配对元数据 → 调用正式目录接口 → 保存手机缓存。
    /// 预期结果：指定空间/区域/卡片存在；不存在 thread_id；无需独立规划服务。
    @MainActor func testPrepareLocalPairedSimulatorAndReadRealCatalog() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else {
            throw XCTSkip("仅由隔离工作空间验收提供配对 fixture；真实相机扫码由用户验收")
        }
        struct Fixture: Decodable {
            struct Mac: Decodable { let deviceID: String; let token: String }
            let machineID: String; let certificateSHA256: String; let endpoints: [URL]
            let namespace: String; let mac: Mac; let workspaceID: String; let areaID: String; let nodeID: UInt64
        }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let machine = try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: [
            "id": fixture.machineID, "name": "群聊本地验收 Mac", "certificateSHA256": fixture.certificateSHA256,
            "endpoints": fixture.endpoints.map(\.absoluteString)
        ]))
        let credential = DeviceCredential(deviceID: fixture.mac.deviceID, token: fixture.mac.token)
        let query: [CFString: Any] = [kSecClass: kSecClassGenericPassword, kSecAttrService: "com.norma-ai.os.ios.device",
                                     kSecAttrAccount: machine.id, kSecAttrSynchronizable: false]
        var item = query
        item[kSecValueData] = try JSONEncoder().encode(credential)
        item[kSecAttrAccessible] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = SecItemAdd(item as CFDictionary, nil)
        if status == errSecDuplicateItem {
            XCTAssertEqual(SecItemUpdate(query as CFDictionary, [kSecValueData: try JSONEncoder().encode(credential)] as CFDictionary), errSecSuccess, "只更新本验收 Mac 凭据")
        } else { XCTAssertEqual(status, errSecSuccess, "配对 fixture 必须可保存在真实模拟器 Keychain") }
        let computers = ComputerModel()
        try await computers.store.save(machine)
        UserDefaults.standard.set(machine.id, forKey: "selected-machine")
        await computers.load()
        let api = DirectWorkspaceChatAPI(machine: machine, client: computers.client)
        let catalog = try await api.catalog()
        let workspace = try XCTUnwrap(catalog.workspaces.first { $0.id == fixture.workspaceID })
        XCTAssertTrue(workspace.projects.contains { $0.areaID == fixture.areaID }, "目录必须使用真实桌面项目区域")
        XCTAssertTrue(workspace.agents.contains { $0.agent.card.nodeID == fixture.nodeID }, "冷卡片必须能作为精确派工目标")
        XCTAssertFalse(catalog.workspaces.contains { $0.id == "thread" }, "不得合成独立工作线程")
        let model = WorkspaceChatModel()
        try await model.store.saveCatalog(catalog)
        try await model.load(macID: machine.id)
        XCTAssertEqual(model.workspaces.count, catalog.workspaces.count, "App 使用正式共享目录缓存")
    }
}

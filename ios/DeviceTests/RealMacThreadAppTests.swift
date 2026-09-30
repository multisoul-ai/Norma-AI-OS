import XCTest
import Security
@testable import NormaCore

private struct RealThreadUIIdentity: Decodable {
    let port: UInt16
    let namespace: String
    let root: String
    let machineID: String
    let endpoints: [URL]
    let cloud: CloudDeviceCredential
    let mac: DeviceCredential
    var cloudService: String { "com.norma-ai.os.ios.cloud.local.\(port).\(namespace)" }
    var ownership: URL { URL(fileURLWithPath: root).appendingPathComponent("ui-identity-owned") }
    static func load() throws -> Self {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "thread-real-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_THREAD_REAL_FIXTURE"] else {
            throw XCTSkip("仅用于显式真实 Mac/Codex 原生验收")
        }
        let value = try JSONDecoder().decode(Self.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        guard UUID(uuidString: value.namespace) != nil,
              !value.endpoints.isEmpty,
              value.endpoints.allSatisfy({ $0.scheme == "wss" && $0.host == "127.0.0.1" }) else {
            throw MobileError.invalidResponse
        }
        return value
    }
}

final class RealMacThreadAppTests: XCTestCase {
    /// 将本轮真实 Mac 和正式网关的隔离身份装入模拟器；不创建任务或结果。
    /// 数据构造：1台临时Mac、1份临时云准入、随机UUID命名空间；既有用户电脑保持不变。
    /// 执行过程：拒绝覆盖配对/工作空间→添加两份凭据与Mac元数据→通过正式中继查询快照。
    /// 预期结果：存在本轮Mac身份且尚无卡片；Workspace、指令和执行必须由后续UI产生。
    func testPrepareRealThreadUI() async throws {
        let fixture = try RealThreadUIIdentity.load()
        let vault = CredentialVault()
        let prior = try await vault.load(machineID: fixture.machineID)
        XCTAssertNil(prior, "不能覆盖已有电脑配对")
        let machines = MachineStore(url: URL.applicationSupportDirectory.appendingPathComponent("Norma/machines.json"))
        let priorMachines = try await machines.load()
        let existingMachine = priorMachines.first { $0.id == fixture.machineID }
        XCTAssertNil(existingMachine, "即使凭据已删除，也不能覆盖已有电脑元数据")
        let directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/UITestWorkspaces/\(fixture.namespace)")
        guard prior == nil, existingMachine == nil, !FileManager.default.fileExists(atPath: directory.path) else {
            XCTFail("真实验收必须使用全新隔离状态"); return
        }
        let query: [CFString: Any] = [kSecClass: kSecClassGenericPassword,
            kSecAttrService: "com.norma-ai.os.ios.device", kSecAttrAccount: fixture.machineID,
            kSecAttrSynchronizable: false, kSecAttrAccessible: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            kSecValueData: try JSONEncoder().encode(fixture.mac)]
        let added = SecItemAdd(query as CFDictionary, nil)
        XCTAssertEqual(added, errSecSuccess, "仅保存本轮临时配对")
        guard added == errSecSuccess else { throw MobileError.keychain(added) }
        try Data().write(to: fixture.ownership, options: .atomic)
        let machine = try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: [
            "id": fixture.machineID, "name": "真实派工验收 Mac", "certificateSHA256": String(fixture.machineID.dropFirst(4)),
            "endpoints": fixture.endpoints.map(\.absoluteString)]))
        try await machines.save(machine)
        try await CloudCredentialVault(service: fixture.cloudService).save(fixture.cloud)
        let connection = try await CollaborationRelayClient(localPort: fixture.port).connect(machine, cloud: fixture.cloud, vault: vault)
        do {
            let snapshot = try await connection.snapshot()
            XCTAssertEqual(snapshot.projects.count, 1, "隔离Mac只能展示本轮授权项目")
            XCTAssertEqual(snapshot.projects.first?.cardCount, 0, "准备阶段不得派工或伪造完成卡片")
            await connection.close()
        } catch { await connection.close(); throw error }
    }

    /// 清理本次真Agent验收凭据，保留其他电脑及验收证据。
    /// 数据构造：本轮随机Mac身份和独立云Keychain service，各精确定位1份凭据。
    /// 执行过程：按ID删除Mac记录/凭据与本轮云准入→重新读取。
    /// 预期结果：测试身份不存在；用户其他电脑记录逐值不变。
    func testCleanupRealThreadUI() async throws {
        let fixture = try RealThreadUIIdentity.load()
        guard FileManager.default.fileExists(atPath: fixture.ownership.path) else {
            throw XCTSkip("准备阶段未取得本轮身份所有权，不得清理既有身份")
        }
        let store = MachineStore(url: URL.applicationSupportDirectory.appendingPathComponent("Norma/machines.json"))
        let before = try await store.load()
        try await store.remove(id: fixture.machineID)
        let vault = CredentialVault()
        try await vault.remove(machineID: fixture.machineID)
        let cloud = CloudCredentialVault(service: fixture.cloudService)
        try await cloud.remove()
        let remaining = try await store.load()
        let credential = try await vault.load(machineID: fixture.machineID)
        let cloudCredential = try await cloud.load()
        XCTAssertEqual(remaining, before.filter { $0.id != fixture.machineID }, "其他电脑必须原样保留")
        XCTAssertNil(credential, "临时Mac凭据必须移除")
        XCTAssertNil(cloudCredential, "临时云凭据必须移除")
        try FileManager.default.removeItem(at: fixture.ownership)
    }
}

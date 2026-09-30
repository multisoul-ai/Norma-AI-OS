import XCTest
@testable import NormaCore

@MainActor final class MachineStoreTests: XCTestCase {
    /// 多电脑列表原子保存、重开保留；无 token，损坏输入不能静默清空再覆盖。
    ///
    /// 数据构造：隔离目录和两台不同证书身份的电脑；每台仅一个 WSS 地址。
    /// 执行过程：保存 A/B → 重开 → 删除 A → 验证 B → 破坏文件 → 尝试写入。
    /// 预期结果：A 被删除、B 保留；无 secret/token；损坏记录明确失败且原始文件不被覆盖。
    func testPersistentMachinesAndCorruptFileProtection() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("machines.json")
        func machine(_ c: String) throws -> PairedMachine {
            let hex = String(repeating: c, count: 64)
            let data = try JSONSerialization.data(withJSONObject: ["version": 1, "machine_id": "mac-" + hex,
                "machine_name": "Mac " + c, "certificate_sha256": hex, "offer_id": hex, "secret": hex,
                "endpoints": ["wss://127.0.0.1:9443/mobile"], "expires_in_seconds": 120])
            return PairedMachine(offer: try PairingOffer.decode(data))
        }
        let a = try machine("a"), b = try machine("b")
        let store = MachineStore(url: url)
        try await store.save(a)
        try await store.save(b)
        let reopened = MachineStore(url: url)
        let both = try await reopened.load()
        XCTAssertEqual(both.map(\.id), [a.id, b.id], "重开必须保留两台电脑")
        let raw = try String(contentsOf: url, encoding: .utf8)
        XCTAssertFalse(raw.contains("token"), "设备凭据不能落入元数据文件")
        XCTAssertFalse(raw.contains("secret"), "一次性 QR secret 不应持久化")
        try await reopened.remove(id: a.id)
        let kept = try await reopened.load()
        XCTAssertEqual(kept.map(\.id), [b.id], "仅删除 A，不波及 B")
        try Data("corrupt".utf8).write(to: url)
        do { try await reopened.save(a); XCTFail("损坏文件不能被新机器覆盖") } catch {}
        XCTAssertEqual(try String(contentsOf: url, encoding: .utf8), "corrupt", "必须保留原始损坏数据供恢复")
    }

    /// 电脑离线时只能显示上次真实同步的项目摘要，重新配对不能抹掉该时间与来源。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   1 台测试 Mac、1 个项目「iOS App」、成功同步时间 Unix 1000 秒；没有任何在线状态夹具。
    ///
    /// 执行过程：保存电脑 → 写入一次真实同步摘要 → 较早的迟到响应不能覆盖 → 重建 Store → 用同一二维码元数据再保存。
    ///
    /// 预期结果：同步时间和项目名跨重启及重新配对保留；未同步项目不会凭空出现。
    func testLastSuccessfulProjectSyncSurvivesRestartAndPairingRefresh() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: dir) }
        let hex = String(repeating: "a", count: 64)
        let data = try JSONSerialization.data(withJSONObject: ["version": 1, "machine_id": "mac-" + hex,
            "machine_name": "Alan Studio", "certificate_sha256": hex, "offer_id": hex, "secret": hex,
            "endpoints": ["wss://127.0.0.1:9443/mobile"], "expires_in_seconds": 120])
        let machine = PairedMachine(offer: try PairingOffer.decode(data))
        let store = MachineStore(url: dir.appendingPathComponent("machines.json"))
        try await store.save(machine)
        let syncedAt = Date(timeIntervalSince1970: 1_000)
        try await store.updateObservation(id: machine.id, projects: [KnownProject(id: "project-a", name: "iOS App")], at: syncedAt)
        // 迟到响应 900 秒 < 已记录的 1000 秒，不能用过时项目替换最近真实目录。
        try await store.updateObservation(id: machine.id, projects: [KnownProject(id: "project-b", name: "旧 API")],
                                          at: Date(timeIntervalSince1970: 900))

        let reopened = MachineStore(url: dir.appendingPathComponent("machines.json"))
        let firstLoad = try await reopened.load()
        let first = try XCTUnwrap(firstLoad.first)
        XCTAssertEqual(first.lastSyncedAt, syncedAt, "最近成功同步时间必须跨重启保留")
        XCTAssertEqual(first.knownProjects?.map(\.name), ["iOS App"], "离线页只可显示真实读取过的项目")
        XCTAssertFalse(first.knownProjects?.contains(where: { $0.id == "project-b" }) == true, "未同步的项目不能出现在缓存中")

        try await reopened.save(machine)
        let refreshedLoad = try await reopened.load()
        let refreshed = try XCTUnwrap(refreshedLoad.first)
        XCTAssertEqual(refreshed.lastSyncedAt, syncedAt, "重新配对不能清空最近同步时间")
        XCTAssertEqual(refreshed.knownProjects?.map(\.id), ["project-a"], "重新配对不能丢失已核验项目摘要")
    }

    /// 升级前电脑元数据缺少项目摘要字段时仍可加载，不能伪造同步结果。
    ///
    /// 数据构造（含关键数值的推导过程）：1 台旧版 Mac；64 位十六进制证书指纹对应 mac- 前缀 ID；
    ///   旧 JSON 仅有 id、name、certificateSHA256 和 1 个 WSS 地址，不含新增的两个可选字段。
    ///
    /// 执行过程：直接写旧 JSON → 从 Store 加载 → 核对身份和未同步状态。
    ///
    /// 预期结果：旧电脑仍可使用；同步时间与目录均为空，不能显示原型夹具项目。
    func testLegacyMachineRecordLoadsWithoutInventingProjects() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: dir) }
        let hex = String(repeating: "c", count: 64)
        let url = dir.appendingPathComponent("machines.json")
        let old = [["id": "mac-" + hex, "name": "旧 Mac", "certificateSHA256": hex,
                    "endpoints": ["wss://127.0.0.1:9443/mobile"]] as [String: Any]]
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try JSONSerialization.data(withJSONObject: old).write(to: url)

        let loaded = try await MachineStore(url: url).load()
        XCTAssertEqual(loaded.count, 1, "旧元数据须保留唯一电脑")
        let machine = try XCTUnwrap(loaded.first)
        XCTAssertEqual(machine.id, "mac-" + hex, "旧配对身份不得迁移成新 ID")
        XCTAssertNil(machine.lastSyncedAt, "旧文件不能凭空产生成功同步时间")
        XCTAssertNil(machine.knownProjects, "旧文件不能凭空显示项目")
    }
}

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
}

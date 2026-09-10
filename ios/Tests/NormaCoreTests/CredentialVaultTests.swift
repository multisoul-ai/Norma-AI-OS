import XCTest
import Security
@testable import NormaCore

@MainActor final class CredentialVaultTests: XCTestCase {
    /// 手机凭据保存在原生 Keychain，重复配对沿用原凭据；另一电脑不能读取，删除后不能恢复旧 token。
    ///
    /// 数据构造：独立测试 service + 两个机器 ID；系统随机 token 为 32 字节，即 64 hex 字符。
    /// 执行过程：A 保存 → 再读 A → 查询 B → 删除 A → 重新查询 A；defer 仅清理本测试 service。
    /// 预期结果：A 身份/token 稳定；B 和已删除 A 均无记录；调试输出无 token。
    func testNativeKeychainScopesCredentialsToMachine() async throws {
        let service = "com.norma.test." + UUID().uuidString
        defer { SecItemDelete([kSecClass: kSecClassGenericPassword, kSecAttrService: service] as CFDictionary) }
        let vault = CredentialVault(service: service)
        let a = try await vault.loadOrCreate(machineID: "mac-" + String(repeating: "a", count: 64))
        let again = try await vault.loadOrCreate(machineID: "mac-" + String(repeating: "a", count: 64))
        XCTAssertEqual(a.deviceID, again.deviceID, "重试不能换手机身份")
        XCTAssertEqual(a.token, again.token, "丢失响应后必须沿用原凭据回查")
        XCTAssertEqual(a.token.count, 64, "随机 256 bit 凭据必须完整保存")
        XCTAssertFalse(String(describing: a).contains(a.token), "调试不能输出 token")
        let b = try await vault.load(machineID: "mac-" + String(repeating: "b", count: 64))
        XCTAssertNil(b, "A 的凭据不能用于 B")
        try await vault.remove(machineID: "mac-" + String(repeating: "a", count: 64))
        let removed = try await vault.load(machineID: "mac-" + String(repeating: "a", count: 64))
        XCTAssertNil(removed, "删除后不能恢复旧 token")
    }
}

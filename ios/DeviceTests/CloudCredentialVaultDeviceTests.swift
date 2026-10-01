import XCTest
import NormaCore

final class CloudCredentialVaultDeviceTests: XCTestCase {
    /// 新安装的手机尚无云端邀请时，独立 Keychain 查询应返回空，而不是阻止用户看到邀请入口。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   1 个随机 service 命名空间；已有条目为 0，所以查询结果应为 nil。
    ///
    /// 执行过程：创建独立 vault → 查询未存在的设备凭据。
    ///
    /// 预期结果：查询成功且结果为空；不凭空生成设备令牌。
    func testUninvitedPhoneCanReadEmptyCloudKeychain() async throws {
        let vault = CloudCredentialVault(service: "com.norma-ai.os.ios.cloud.tests.\(UUID().uuidString)")
        let credential = try await vault.load()
        XCTAssertNil(credential, "未受邀手机的 Keychain 查询应为空，不应报错或生成凭据")
    }
}

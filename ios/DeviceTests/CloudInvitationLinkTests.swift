import XCTest
@testable import Norma
@testable import NormaCore

final class CloudInvitationLinkTests: XCTestCase {
    /// 本地TestFlight构建配置必须被真实App模型使用，并在重启模型后恢复同一设备授权。
    /// 数据构造：Node提供随机namespace、1张短期证书和1个一次性邀请；测试不注入客户端或设备令牌。
    /// 执行过程：读取打包Info.plist→默认模型兑换真实HTTPS邀请→重建模型读取Keychain→真实WSS查询不存在的Mac。
    /// 预期结果：身份持久化且无伪造回复；未注册Mac不能假装在线；最后只清理本轮独立凭据。
    @MainActor func testBundledLocalInvitationPersistsThroughModelReload() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_LOCAL_TLS_FIXTURE"], !path.isEmpty else {
            throw XCTSkip("由本地TLS原生验收提供隔离配置")
        }
        struct Fixture: Decodable { let namespace: String; let code: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let local = try XCTUnwrap(LocalCloudConfiguration.bundled(), "App包必须包含显式TLS配置")
        XCTAssertEqual(local.namespace, fixture.namespace, "测试不能覆盖其他本地设备身份")
        guard local.namespace == fixture.namespace else { return }
        let vault = CloudCredentialVault(service: "com.norma-ai.os.ios.cloud.local.\(local.namespace).\(local.certificateSHA256)")
        let prior = try await vault.load()
        XCTAssertNil(prior, "验收命名空间必须全新")
        guard prior == nil else { return }
        addTeardownBlock { try await vault.remove() }
        let model = CloudChatModel()
        await model.load(); await model.claim(code: fixture.code)
        XCTAssertNil(model.error, "默认模型应经打包配置成功兑换真实邀请")
        let credential = try XCTUnwrap(model.credential, "服务必须返回真实设备身份")
        let restored = CloudChatModel()
        await restored.load()
        XCTAssertEqual(restored.credential?.deviceID, credential.deviceID, "模型重建后必须从独立Keychain恢复")
        XCTAssertTrue(restored.messages.isEmpty, "仅准入不得产生Norma回复或派工")
        let paired = DeviceCredential(deviceID: UUID().uuidString, token: String(repeating: "c", count: 64))
        do {
            let connection = try await CollaborationRelayClient(local: local).connect(machineID: "mac-" + String(repeating: "a", count: 64), cloud: credential, credential: paired)
            await connection.close()
            XCTFail("未注册Mac不能建立可操作连接")
        } catch MobileError.connectionFailed { }
    }

    /// 邀请深链只接受 Norma 自有 scheme 和固定 invite 主机，不能把外部链接当作云服务配置。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   32 字节随机邀请以 base64url 表示为 43 字符；测试用 43 个 A 模拟合法编码长度。
    ///   另给出 2 个错误目的地：外部 HTTPS 主机和 Norma 内的其他路径。
    ///
    /// 执行过程：解析有效链接 → 解析外部地址 → 解析错误路径 → 解析短邀请码。
    ///
    /// 预期结果：只有固定深链返回邀请码；错误目的地与短码都没有可兑换的邀请码。
    func testInvitationLinkRejectsForeignDestinations() {
        let code = String(repeating: "A", count: 43)
        XCTAssertEqual(CloudInvitationLink.code(from: URL(string: "norma://invite?code=\(code)")!), code,
                       "固定 Norma 邀请深链应提取完整的一次性邀请码")
        XCTAssertNil(CloudInvitationLink.code(from: URL(string: "https://example.com/invite?code=\(code)")!),
                     "外部 HTTPS 链接不能控制云服务准入")
        XCTAssertNil(CloudInvitationLink.code(from: URL(string: "norma://other?code=\(code)")!),
                     "Norma 的其他深链不能被当成邀请")
        XCTAssertNil(CloudInvitationLink.code(from: URL(string: "norma://invite?code=short")!),
                     "长度错误的邀请码不能进入兑换流程")
    }
}

import XCTest
@testable import NormaCore

final class LocalCloudConfigurationTests: XCTestCase {
    /// 同一证书pin覆盖邀请HTTP及派工WSS；错误证书不得发送令牌或消耗邀请。
    /// 数据构造：Node创建1张一天有效证书及1个邀请；反例pin为64个0；未注册Mac为64个a。
    /// 执行过程：先用错pin尝试兑换→正确pin兑换→重复兑换拒绝→用真实设备凭据开WSS。
    /// 预期结果：错pin失败、邀请仍可兑换、重复邀请失败；WSS返回未注册Mac离线，不能虚报成功。
    func testPinnedInvitationAndRelayUseRealTLS() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_LOCAL_TLS_FIXTURE"], !path.isEmpty else {
            throw XCTSkip("由check-local-tls.mjs提供本机TLS网关")
        }
        struct Fixture: Decodable { let origin: String; let pin: String; let namespace: String; let code: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let info = ["NormaLocalGatewayOrigin": fixture.origin, "NormaLocalGatewayPin": fixture.pin,
                    "NormaLocalGatewayNamespace": fixture.namespace]
        let wrong = try XCTUnwrap(LocalCloudConfiguration(info: info.merging(["NormaLocalGatewayPin": String(repeating: "0", count: 64)]) { _, new in new }))
        do {
            _ = try await CloudTextClient(local: wrong).claim(code: fixture.code, name: "wrong certificate")
            XCTFail("错误证书必须在HTTP正文发送前拒绝")
        } catch is URLError { }
        let local = try XCTUnwrap(LocalCloudConfiguration(info: info))
        let client = CloudTextClient(local: local)
        let credential = try await client.claim(code: fixture.code, name: "本地真机TLS验收")
        XCTAssertNotNil(UUID(uuidString: credential.deviceID), "正式网关应签发稳定设备身份")
        XCTAssertNotEqual(credential.token, fixture.code, "设备令牌不能复用一次性邀请")
        do {
            _ = try await client.claim(code: fixture.code, name: "duplicate")
            XCTFail("同一邀请不能重复兑换")
        } catch CloudTextError.invitationInvalid { }
        let paired = DeviceCredential(deviceID: String(repeating: "b", count: 64), token: String(repeating: "c", count: 64))
        do {
            let connection = try await CollaborationRelayClient(local: local).connect(machineID: "mac-" + String(repeating: "a", count: 64), cloud: credential, credential: paired)
            await connection.close()
            XCTFail("未注册Mac不能返回可操作连接")
        } catch MobileError.connectionFailed { }
    }

    /// 真机本地验收只能使用显式HTTPS、证书pin和隔离身份，缺项不能退回正式服务。
    /// 数据构造：192.168.1.8为RFC1918私网；9444为独立TLS入口；64个a组成SHA256格式pin；1个UUID隔离凭据。
    /// 执行过程：解析完整构建配置，再逐项破坏协议、地址、pin和身份，检查拒绝行为。
    /// 预期结果：正确配置保留原目标；HTTP、公网、缺pin、缺namespace、伪URL均不得进入客户端。
    func testPhysicalDeviceConfigurationRequiresPinnedPrivateHTTPS() throws {
        let namespace = UUID().uuidString
        let pin = String(repeating: "a", count: 64)
        let info = ["NormaLocalGatewayOrigin": "https://192.168.1.8:9444/",
                    "NormaLocalGatewayPin": pin, "NormaLocalGatewayNamespace": namespace]
        XCTAssertNil(try LocalCloudConfiguration(info: [:]), "普通安装不得启用本地覆盖")
        let config = try XCTUnwrap(LocalCloudConfiguration(info: info))
        XCTAssertEqual(config.origin.absoluteString, "https://192.168.1.8:9444/", "只使用编译时明确的私网入口")
        XCTAssertEqual(config.certificateSHA256, pin, "TLS必须核对完整证书pin")
        XCTAssertEqual(config.namespace, namespace, "开发授权必须隔离于正式凭据")
        XCTAssertThrowsError(try LocalCloudConfiguration(info: info.merging(["NormaLocalGatewayOrigin": "http://192.168.1.8:9444/"]) { _, new in new }), "真机不能明文发送设备令牌")
        XCTAssertThrowsError(try LocalCloudConfiguration(info: info.merging(["NormaLocalGatewayOrigin": "https://example.com/"]) { _, new in new }), "本地测试不能意外连接公网")
        XCTAssertThrowsError(try LocalCloudConfiguration(info: info.merging(["NormaLocalGatewayOrigin": "https://192.168.1.8:9444/?next=evil"]) { _, new in new }), "不接受查询参数改变路由")
        XCTAssertThrowsError(try LocalCloudConfiguration(info: info.merging(["NormaLocalGatewayPin": ""]) { _, new in new }), "不能缺失pin后使用系统默认认证")
        XCTAssertThrowsError(try LocalCloudConfiguration(info: info.merging(["NormaLocalGatewayNamespace": ""]) { _, new in new }), "不能混用正式历史及Keychain")
    }
}

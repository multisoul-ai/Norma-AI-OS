import XCTest
@testable import NormaCore

@MainActor final class PushSubscriptionTests: XCTestCase {
    /// A电脑的订阅状态不能在切换B后提交，即使两台电脑版本都为0。
    ///
    /// 数据构造：两个64位证书指纹分别为a/b；A订阅版本0；B使用独立空Keychain。
    /// 执行过程：解码A状态 → 对B执行关闭 → 必须在凭据读取和网络连接前拒绝。
    /// 预期结果：错误为wrongComputer；不能退化为invalidCredential或发出关闭请求。
    func testSubscriptionCannotFollowMachineSwitch() async throws {
        let a = "mac-" + String(repeating: "a", count: 64)
        let fingerprint = String(repeating: "b", count: 64)
        let state = try JSONDecoder().decode(PushSubscriptionState.self, from: JSONSerialization.data(withJSONObject: [
            "machine_id": a, "device_id": UUID().uuidString, "revision": 0,
            "registration": NSNull(), "registered_at_ms": NSNull()
        ]))
        let machine = try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: [
            "id": "mac-" + fingerprint, "name": "B", "certificateSHA256": fingerprint,
            "endpoints": ["wss://127.0.0.1:1/mobile"]
        ]))
        let client = MobileClient(vault: CredentialVault(service: "com.norma.push-test." + UUID().uuidString))
        do {
            _ = try await client.updatePushSubscription(machine, expected: state, registration: nil)
            XCTFail("不能将A订阅用于B电脑")
        } catch {
            XCTAssertEqual(error as? MobileError, .wrongComputer, "切换电脑必须在凭据和网络前拒绝旧状态")
            XCTAssertNotEqual(error as? MobileError, .invalidCredential, "不能把身份串线误报为未配对")
        }
    }

    /// APNs环境、token与订阅版本必须严格校验，缺失或矛盾响应不能声称通知已启用。
    ///
    /// 数据构造：32字节=64位hex；旧订阅版本0与启用后的版本1；空token和非hex均非法。
    /// 执行过程：验证有效token和Wire回执 → 拒绝非法token、未知环境及无注册时间的启用态。
    /// 预期结果：只接受明确有效订阅；状态关闭不带token，日志描述不暴露token。
    func testSubscriptionValidationAndWireErrors() throws {
        let registration = APNsRegistration(token: String(repeating: "a", count: 64), environment: .sandbox)
        XCTAssertNoThrow(try registration.validate(), "标准APNs token必须可注册")
        XCTAssertThrowsError(try APNsRegistration(token: "", environment: .sandbox).validate(), "空token不能表示启用")
        XCTAssertThrowsError(try APNsRegistration(token: "../device", environment: .production).validate(), "token不能构造路径")
        XCTAssertFalse(String(describing: registration).contains(registration.token), "token不得进入日志")
        let json = #"{"version":1,"request_id":"p","result":{"status":"push_subscription","subscription":{"machine_id":"mac-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","device_id":"phone-a","revision":1,"registration":{"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","environment":"sandbox"},"registered_at_ms":1800000000000}}}"#
        let state = try XCTUnwrap(WireReply.decode(Data(json.utf8), expectedID: "p").result?.subscription)
        XCTAssertNoThrow(try state.validate(), "启用态需要明确版本和注册时间")
        XCTAssertEqual(state.registration, registration, "应保留准确环境和token")
        let malformed = json.replacingOccurrences(of: "1800000000000", with: "null")
        let invalid = try XCTUnwrap(WireReply.decode(Data(malformed.utf8), expectedID: "p").result?.subscription)
        XCTAssertThrowsError(try invalid.validate(), "没有注册时间不能判断APNs失效先后")
        XCTAssertThrowsError(try WireReply.decode(Data(json.replacingOccurrences(of: "sandbox", with: "unknown").utf8), expectedID: "p"), "未知环境必须拒绝")
        let changed = #"{"version":1,"request_id":"p","error":"push_subscription_changed"}"#
        XCTAssertThrowsError(try WireReply.decode(Data(changed.utf8), expectedID: "p")) { error in
            XCTAssertEqual(error as? MobileError, .pushSubscriptionChanged, "竞争时必须提示重新核验，不能自动覆盖")
        }
    }
}

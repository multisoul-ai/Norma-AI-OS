import XCTest
import CoreImage
import UIKit
@testable import Norma
@testable import NormaCore

final class PairedCloudRegistrationTests: XCTestCase {
    /// 一张 Mac 二维码完成配对与服务注册；重建模型后可直接经中继访问同一 Mac。
    /// 数据构造：全新 Keychain 命名空间、真实二维码图像、真实 Rust TLS Mac 和 Node 服务；不注入云邀请码或令牌。
    /// 执行过程：图像解码→正式配对→等待 Mac 批准→App 注册→重建模型→真实中继读取→重复注册。
    /// 预期结果：只有一个云身份，未注册前为空；注册后长期保存，不再依赖第二码，也不生成虚构对话。
    @MainActor func testOneMacQRCodeRegistersAndPersists() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_REGISTRATION_FIXTURE"], !path.isEmpty else {
            throw XCTSkip("由真实 Mac 注册验收提供独立服务")
        }
        struct Fixture: Decodable { let offer: String; let port: UInt16; let namespace: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let filter = try XCTUnwrap(CIFilter(name: "CIQRCodeGenerator"))
        filter.setValue(Data(fixture.offer.utf8), forKey: "inputMessage")
        let output = try XCTUnwrap(filter.outputImage?.transformed(by: .init(scaleX: 8, y: 8)))
        let image = try XCTUnwrap(CIContext().createCGImage(output, from: output.extent))
        let png = try XCTUnwrap(UIImage(cgImage: image).pngData())
        let decoded = try PairingImageCode.decode(png)
        XCTAssertEqual(decoded, fixture.offer, "正式图片扫码路径必须读出原 Mac 二维码")
        let offer = try PairingOffer.decode(Data(decoded.utf8)), machine = PairedMachine(offer: offer)
        let macVault = CredentialVault(service: "norma.registration.mac.\(fixture.namespace)")
        let cloudVault = CloudCredentialVault(service: "norma.registration.cloud.\(fixture.namespace)")
        addTeardownBlock {
            try await macVault.remove(machineID: machine.id)
            try await cloudVault.remove(); try await cloudVault.pendingRegistration.remove()
        }
        let store = CloudChatStore(databaseURL: URL.temporaryDirectory.appendingPathComponent("\(fixture.namespace).sqlite"))
        let client = MobileClient(vault: macVault)
        let cloud = CloudChatModel(store: store, vault: cloudVault, client: try CloudTextClient(localPort: fixture.port))
        await cloud.load()
        XCTAssertNil(cloud.credential, "配对前不能伪造服务授权")
        let result = try await client.connect(offer, deviceName: "Single Scan iPhone")
        guard case .pending = result else { return XCTFail("全新手机必须先等待 Mac 确认") }
        // 60 × 100ms = 6秒：本机审批轮询20ms，余量覆盖原生网络调度。
        var granted = false
        for _ in 0..<60 {
            do { _ = try await client.authenticate(machine); granted = true; break }
            catch MobileError.unauthorized { try await Task.sleep(for: .milliseconds(100)) }
        }
        XCTAssertTrue(granted, "必须等待真实 Mac 审批，不能用云端注册替代电脑权限")
        try await cloud.register(on: machine, using: client)
        let accepted = try XCTUnwrap(cloud.credential, "只扫 Mac 二维码后应自动完成注册")
        let pending = try await cloudVault.pendingRegistration.load()
        XCTAssertNil(pending, "成功后清理待注册凭据")
        let restored = CloudChatModel(store: store, vault: cloudVault, client: try CloudTextClient(localPort: fixture.port))
        await restored.load()
        XCTAssertEqual(restored.credential?.deviceID, accepted.deviceID, "App模型重建必须恢复同一云身份")
        XCTAssertTrue(restored.messages.isEmpty, "注册不得写入虚构聊天消息")
        let relay = try await CollaborationRelayClient(localPort: fixture.port).connect(machine, cloud: accepted, vault: macVault)
        let relayGrant = await relay.grant
        XCTAssertEqual(relayGrant?.allProjectsAccess, "control", "真实中继必须完成 Mac 端权限校验")
        XCTAssertEqual(relayGrant?.revoked, false, "中继不能接受已撤销设备")
        await relay.close()
        try await restored.register(on: machine, using: client)
        XCTAssertEqual(restored.credential?.deviceID, accepted.deviceID, "重试不能生成第二个身份")
    }

    /// 注册网络中断时未完成凭据只存于待注册区，重试复用，不能误报已授权。
    /// 数据构造：随机命名空间；1个32字节随机token编码为43字符，正式凭据区初始为空。
    /// 执行过程：准备待注册→重建Vault读取→加载App模型→核对正式/待注册状态。
    /// 预期结果：重试身份与密钥不变；未获得服务回执之前模型 credential 仍为空。
    @MainActor func testPendingRegistrationIsNotAnAuthorizedCredential() async throws {
        let vault = CloudCredentialVault(service: "norma.registration.pending.\(UUID().uuidString)")
        addTeardownBlock { try await vault.remove(); try await vault.pendingRegistration.remove() }
        let first = try await vault.pendingRegistration.prepareRegistration()
        let retry = try await vault.pendingRegistration.prepareRegistration()
        XCTAssertEqual(first.deviceID, retry.deviceID, "重试应复用之前的身份")
        XCTAssertEqual(first.token, retry.token, "回包丢失不能改变密钥")
        XCTAssertEqual(first.token.count, 43, "32字节密钥的base64url无填充长度为43")
        let active = try await vault.load()
        XCTAssertNil(active, "准备注册不等于服务已授权")
    }
}

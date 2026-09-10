import XCTest
@testable import NormaCore

final class PairingOfferTests: XCTestCase {
    /// 扫码必须精确绑定机器和证书，拒绝明文地址、URL注入与不受支持版本。
    ///
    /// 数据构造：一个 v1 QR；证书指纹和 secret 都为 64 hex 字符（32 字节）。
    /// 执行过程：读取有效QR → 改为另一机器 → 改ws明文 → 注入query → 改协议版本。
    /// 预期结果：有效 QR 可读且调试不输出 secret；其余输入均拒绝，不能退化为未校验连接。
    func testOfferRequiresPinnedMachineAndEncryptedEndpoints() throws {
        var value: [String: Any] = ["version": 1, "machine_id": "mac-" + String(repeating: "a", count: 64),
            "machine_name": "MacBook Pro", "certificate_sha256": String(repeating: "a", count: 64),
            "offer_id": String(repeating: "b", count: 64), "secret": String(repeating: "c", count: 64),
            "endpoints": ["wss://192.168.1.2:9443/mobile"], "expires_in_seconds": 120]
        func data() throws -> Data { try JSONSerialization.data(withJSONObject: value) }
        let offer = try PairingOffer.decode(data())
        XCTAssertEqual(offer.machineName, "MacBook Pro", "保留可见电脑名称")
        XCTAssertFalse(String(describing: offer).contains(String(repeating: "c", count: 64)), "不可输出二维码 secret")
        value["machine_id"] = "mac-" + String(repeating: "b", count: 64)
        XCTAssertThrowsError(try PairingOffer.decode(data()), "机器身份与 pin 不一致必须拒绝")
        XCTAssertThrowsError(try JSONDecoder().decode(PairingOffer.self, from: data()), "直接 Decodable 入口也不能绕过身份校验")
        value["machine_id"] = offer.machineID
        value["endpoints"] = ["ws://192.168.1.2:9443/mobile"]
        XCTAssertThrowsError(try PairingOffer.decode(data()), "不能允许明文设备凭据")
        value["endpoints"] = ["wss://192.168.1.2:9443/mobile?token=leak"]
        XCTAssertThrowsError(try PairingOffer.decode(data()), "凭据不能进入 URL")
        value["endpoints"] = ["wss://192.168.1.2:9443/mobile"]
        value["version"] = 2
        XCTAssertThrowsError(try PairingOffer.decode(data()), "不猜测下一版协议")
    }
}

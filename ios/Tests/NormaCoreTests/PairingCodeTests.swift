import XCTest
@testable import NormaCore

final class PairingCodeTests: XCTestCase {
    /// 验证码保留前导零；地址只接受一台电脑，不允许协议降级或凭据注入。
    ///
    /// 数据构造：默认端口 9443 与 Mac 一致；六位码 012345；LAN、Tailscale DNS 与 IPv6 三类入口。
    /// 执行过程：解析简写地址/完整地址 → 校验六位数字 → 拒绝 URL 凭据、query、非 WSS 与不合法端口。
    /// 预期结果：合法地址归一为 /mobile；错误输入不建立网络连接，也不能被截断成合法码。
    func testAddressAndSixDigitsAreValidatedBeforeNetwork() throws {
        XCTAssertEqual(try PairingCode.endpoint("192.168.1.2").absoluteString, "wss://192.168.1.2:9443/mobile", "默认端口应与桌面一致")
        XCTAssertEqual(try PairingCode.endpoint("mac.tail.ts.net:9443").host, "mac.tail.ts.net", "支持 Tailscale DNS")
        XCTAssertEqual(try PairingCode.endpoint("[fd7a:115c:a1e0::1]:9443").port, 9443, "支持带括号 IPv6")
        XCTAssertEqual(try PairingCode.endpoint("wss://127.0.0.1:1234/mobile").port, 1234, "保留显式端口")
        XCTAssertTrue(PairingCode.validCode("012345"), "前导零是验证码的一部分")
        XCTAssertFalse(PairingCode.validCode("12345"), "五位码不能提交")
        XCTAssertFalse(PairingCode.validCode("1234567"), "不可截断多余数字")
        XCTAssertFalse(PairingCode.validCode("１２３４５６"), "仅接受 ASCII 数字")
        XCTAssertThrowsError(try PairingCode.endpoint("ws://192.168.1.2:9443/mobile"), "拒绝协议降级")
        XCTAssertThrowsError(try PairingCode.endpoint("wss://secret@192.168.1.2/mobile"), "地址不得夹带凭据")
        XCTAssertThrowsError(try PairingCode.endpoint("192.168.1.2/mobile?pin=123456"), "验证码不能进入 URL")
        XCTAssertThrowsError(try PairingCode.endpoint("192.168.1.2:0"), "不能连接保留端口零")
        XCTAssertThrowsError(try PairingCode.endpoint(""), "空地址不猜测目标电脑")
    }
}

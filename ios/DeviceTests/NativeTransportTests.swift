import XCTest
import UIKit

final class NativeTransportTests: XCTestCase {
    /// 真实 iPhone 构建必须包含两种 Provider 的独立矢量图标，避免静默回退为通用终端图标。
    ///
    /// 数据构造：读取实际 app asset catalog 的 Codex / Claude，尺寸均为原始 24×24pt。
    /// 执行过程：从 UIKit 读取编译后的资源 → 检查两者独立存在 → 检查不存在的图标不被错误匹配。
    /// 预期结果：两种品牌资源均可解码为图像，且资源查找不会把未知 Provider 映射成 Codex。
    func testProviderAssets() {
        XCTAssertNotNil(UIImage(named: "Codex"), "必须打包 Codex 的品牌图标")
        XCTAssertNotNil(UIImage(named: "Claude"), "必须打包 Claude 的品牌图标")
        XCTAssertNil(UIImage(named: "not-a-provider"), "未知 Provider 不得伪装成已有品牌")
    }
    /// iPhone App 构建产物必须保留设备范围、版本和隐私用途，不能带全局 ATS 例外。
    ///
    /// 数据构造：读取实际测试 host 的 Info.plist，device family 1 表示 iPhone，2 表示 iPad。
    /// 执行过程：从 Bundle.main 取产物元数据 → 验证设备族与版本 → 检查隐私和 ATS。
    /// 预期结果：仅包含 iPhone、有版本和相机用途；不能意外生成 iPad 或禁用 ATS。
    func testAppManifest() {
        let info = Bundle.main.infoDictionary ?? [:]
        XCTAssertEqual(info["UIDeviceFamily"] as? [Int], [1], "首版 iPhone 范围不能被 XcodeGen 默认值改成 iPad")
        XCTAssertEqual(info["CFBundleShortVersionString"] as? String, "0.1.0", "产物必须有可识别版本")
        let build = info["CFBundleVersion"] as? String ?? ""
        XCTAssertNotNil(build.range(of: "^[1-9][0-9]*$", options: .regularExpression), "构建号必须为正整数；发布流程另行核验递增，不能锁死历史构建 2")
        XCTAssertNotNil(info["NSCameraUsageDescription"], "相机请求必须说明用途")
        XCTAssertNil(info["NSAppTransportSecurity"], "pin 校验不能通过全局 ATS 例外实现")
    }

    /// iOS 的 URLSession/Keychain 与真实 Rust TLS 服务互通，复用 macOS probe 的安全检查。
    ///
    /// 数据构造：Rust fixture 提供受限临时 QR 文件与只读项目；未提供 fixture 时显式跳过。
    /// 执行过程：错误证书拒绝，或真实配对 → Mac 批准 → 认证范围检查 → Mac 撤销 → 再认证拒绝。
    /// 预期结果：相同检查在实际 iOS simulator XCTest host 内通过，不以 macOS Foundation 代替。
    func testRustPairingTransport() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard let path = environment["NORMA_TEST_QR"], path.hasPrefix("/"),
              let mode = environment["NORMA_TEST_MODE"], ["terminal", "archive", "code", "feedback", "normal", "wrong-pin", "conversation", "messages", "interrupts", "queues", "approvals", "bash-edges", "approval-stop", "write-approvals", "external", "events", "large", "chat-events", "plans", "todos", "reviews"].contains(mode) else {
            throw XCTSkip("需使用 scripts/check-norma-ios-transport.sh 的 Rust 隔离 fixture")
        }
        try await PairingCheck.run(qrURL: URL(fileURLWithPath: path), mode: mode)
    }
}

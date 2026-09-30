import XCTest
import UIKit

final class ConnectedComputersFlowTests: XCTestCase {
    /// 单码注册页跨两层 Sheet 仍能读取共享云注册状态，不能因环境未注入而闪退。
    /// 数据构造：真实 App 启动，不预注入云凭据；5秒=启动和系统转场余量，不等待网络。
    /// 执行过程：更多模块→已连接电脑→添加电脑→扫码图片入口→取消→返回电脑页。
    /// 预期结果：出现自动注册说明且页面存活；取消不显示注册成功，也不要求第二张邀请码。
    @MainActor func testSingleScanRegistrationGuideRemainsAlive() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["idea-more-modules"].waitForExistence(timeout: 5), "需要进入真实主页面")
        app.buttons["idea-more-modules"].tap(); app.buttons["已连接电脑"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "电脑列表必须可达")
        app.buttons["add-computer"].tap()
        XCTAssertTrue(app.buttons["scan-qr"].waitForExistence(timeout: 5), "共享注册模型必须穿过全屏页与配对Sheet，不能闪退")
        XCTAssertTrue(app.staticTexts["在 Mac 确认授权后，自动完成手机注册。"].exists, "必须清楚说明一张 Mac 二维码即可注册")
        XCTAssertFalse(app.staticTexts["已连接，可以派工"].exists, "未配对不能伪造注册成功")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "single-scan-guide"; shot.lifetime = .keepAlways; add(shot)
        app.buttons["scan-qr"].tap()
        XCTAssertTrue(app.buttons["scan-qr-photo"].waitForExistence(timeout: 5), "无相机的模拟器也能选择真实二维码图片")
        app.buttons["scanner-close"].tap(); app.buttons["pairing-cancel"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "取消必须返回电脑列表")
        XCTAssertFalse(app.buttons["pairing-done"].exists, "取消不能显示已注册完成")
    }

    /// W02 是浅色全屏页面，状态栏背后不能露出旧深色项目页。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   左上采样点 x = 截图宽度 / 20、y = 高度 / 40，避开时间与 Dynamic Island；
    ///   RGB 亮度下限 220/255，允许浅灰背景和系统抗锯齿。
    ///
    /// 执行过程：从现有电脑入口打开 W02 → 截取真实模拟器 App → 检查状态栏空白处。
    ///
    /// 预期结果：状态栏背景为浅色；不能露出旧页面的黑色背景。
    @MainActor func testConnectedComputersUsesLightStatusBarBackground() throws {
        let app = XCUIApplication()
        app.launch()
        app.buttons["idea-more-modules"].tap()
        app.buttons["已连接电脑"].tap()
        XCTAssertTrue(app.staticTexts["已连接电脑"].waitForExistence(timeout: 5), "W02 标题必须可见")
        let image = try XCTUnwrap(app.screenshot().image.cgImage, "截图必须可供颜色验收")
        let data = try XCTUnwrap(image.dataProvider?.data, "截图像素必须可读")
        let bytes = try XCTUnwrap(CFDataGetBytePtr(data), "截图不能是空像素数据")
        let pixelBytes = image.bitsPerPixel / 8
        XCTAssertGreaterThanOrEqual(pixelBytes, 3, "必须至少有 RGB 三个颜色通道")
        guard pixelBytes >= 3 else { return }
        let offset = image.bytesPerRow * (image.height / 40) + pixelBytes * (image.width / 20)
        let brightness = min(bytes[offset], min(bytes[offset + 1], bytes[offset + 2]))
        XCTAssertGreaterThan(brightness, 220, "W02 状态栏背后应是浅色，不能透出旧深色项目页")
    }

    /// W02 电脑页只展示从已配对 Mac 真实读取的项目，并能进入扫码入口。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   notification-ui 隔离 fixture = 1 台「通知验收 Mac」+ 1 个「通知验收项目」；
    ///   8 秒 = 原生启动、已配对 TLS 快照和页面更新的等待上限。
    ///
    /// 执行过程：打开公共电脑入口 → 等待真实目录 → 核对在线电脑和项目 → 打开扫码配对页 → 返回。
    ///
    /// 预期结果：项目来自真实 Mac；同步元数据不拆字换行；不存在原型示例的 Norma API；扫码入口可达并能返回同一电脑页。
    @MainActor func testConnectedComputersShowsVerifiedProjectAndQRAction() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("需要已配对的隔离 Mac fixture，不能拿原型示例数据冒充真实目录")
        }
        let app = XCUIApplication()
        app.launch()
        app.buttons["idea-more-modules"].tap()
        app.buttons["已连接电脑"].tap()
        XCTAssertTrue(app.staticTexts["已连接电脑"].waitForExistence(timeout: 8), "W02 必须有已连接电脑标题")
        XCTAssertTrue(app.staticTexts["项目来自已连接的电脑"].exists, "页面必须说明项目来源")
        XCTAssertTrue(app.staticTexts["通知验收 Mac"].waitForExistence(timeout: 8), "应显示真实配对 Mac")
        XCTAssertTrue(app.staticTexts["通知验收项目"].waitForExistence(timeout: 8), "应显示该 Mac 的真实项目")
        XCTAssertTrue(app.staticTexts["· Mac 直连"].exists, "在线电脑须说明项目来源与连接方式")
        let projectCount = app.staticTexts["· 1 个项目"]
        XCTAssertTrue(projectCount.exists, "电脑卡必须显示真实项目数")
        XCTAssertLessThanOrEqual(projectCount.frame.height, 20, "标准字号下项目数不能被状态标签挤到两行")
        XCTAssertFalse(app.staticTexts["Norma API"].exists, "不能把原型示例项目写成真实目录")
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "W02-真实项目目录"
        shot.lifetime = .keepAlways
        add(shot)
        let scan = app.buttons["add-computer"]
        XCTAssertTrue(scan.isHittable, "扫码添加电脑必须是可触达的主操作")
        scan.tap()
        XCTAssertTrue(app.buttons["scan-qr"].waitForExistence(timeout: 5), "必须进入现有原生扫码配对流程")
        app.buttons["pairing-cancel"].tap()
        XCTAssertTrue(app.staticTexts["已连接电脑"].waitForExistence(timeout: 5), "取消扫码须返回电脑页")
    }

    /// W02 离线后保留上次同步的真实项目，但不能把缓存当成当前可操作权限。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   同一隔离 fixture = 1 台 Mac + 1 个项目；目录响应关闭后单次请求上限 5 秒，
    ///   12 秒 = 5 秒失败等待 + 7 秒启动、重新载入与 UI 调度余量。
    ///
    /// 执行过程：先在线打开并核对项目 → 关闭 fixture 目录响应 → 重启 App → 检查缓存卡。
    ///
    /// 预期结果：电脑为离线、项目仍可读但按钮禁用；不出现英文相对时间或虚构在线权限。
    @MainActor func testConnectedComputersKeepsCachedProjectOffline() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("需要真实配对的隔离 Mac fixture")
        }
        let offline = URL(fileURLWithPath: path).deletingLastPathComponent().appendingPathComponent("ui-catalog-offline")
        defer { try? FileManager.default.removeItem(at: offline) }
        let app = XCUIApplication()
        app.launch()
        app.buttons["idea-more-modules"].tap()
        app.buttons["已连接电脑"].tap()
        XCTAssertTrue(app.staticTexts["通知验收项目"].waitForExistence(timeout: 8), "断开前必须读到真实项目")
        XCTAssertTrue(app.staticTexts["在线"].waitForExistence(timeout: 8), "断开前必须确认 Mac 在线")
        try Data().write(to: offline, options: .atomic)
        app.terminate()
        app.launch()
        app.buttons["idea-more-modules"].tap()
        app.buttons["已连接电脑"].tap()
        XCTAssertTrue(app.staticTexts["离线"].waitForExistence(timeout: 12), "目录不可达后不能继续显示在线")
        XCTAssertTrue(app.staticTexts["通知验收项目"].exists, "离线仍须可读上次同步的项目")
        XCTAssertTrue(app.staticTexts["· 本机缓存"].exists, "离线项目须标明来自本机缓存")
        let project = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "computer-project-")).firstMatch
        XCTAssertTrue(project.exists, "真实项目仍应在电脑卡中")
        XCTAssertFalse(project.isEnabled, "离线项目不得继续进入或执行")
        XCTAssertFalse(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@ OR label CONTAINS %@", " sec", " min")).firstMatch.exists,
                       "中文界面不能混入英文相对时间")
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "W02-离线缓存项目"
        shot.lifetime = .keepAlways
        add(shot)
    }
}

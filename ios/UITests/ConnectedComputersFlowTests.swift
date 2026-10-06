import XCTest
import UIKit

final class ConnectedComputersFlowTests: XCTestCase {
    /// Mac 授予控制权限后，即使云端注册不可用，也能完成电脑配对。
    ///
    /// 数据构造：独立模拟器、真实二维码图片与证书 pin 的回环 TLS 替身；
    ///   Mac 授权为 control、revision=1；cloud_registration 返回 unavailable，云端端口 1 不提供服务。
    ///   8 秒 = 5 秒传输上限 + 3 秒原生扫码与界面调度余量。
    /// 执行过程：图片扫码 → 连接电脑 → 收到 Mac 授权 → 完成 → 进入后台并恢复 → 再次打开电脑列表。
    /// 预期结果：完成按钮立即可用；没有注册重试或暂停提示；保留电脑，且从未请求云端注册证明。
    @MainActor func testPairingFinishesWithoutCloud() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "pairing-direct-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("需要 pairing-direct.mjs 和导入二维码图片的独立模拟器")
        }
        continueAfterFailure = false
        let requestsURL = URL(fileURLWithPath: path).deletingLastPathComponent().appendingPathComponent("requests.json")
        let previousCount = try JSONDecoder().decode([String].self, from: Data(contentsOf: requestsURL)).count
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_CLOUD_LOCAL_PORT"] = "1"
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        XCTAssertTrue(app.buttons["idea-more-modules"].waitForExistence(timeout: 8), "真实 App 主页面必须可达")
        app.buttons["idea-more-modules"].tap(); app.buttons["已连接电脑"].tap()
        app.buttons["add-computer"].tap(); app.buttons["scan-qr"].tap()
        XCTAssertTrue(app.buttons["scan-qr-photo"].waitForExistence(timeout: 8), "必须通过生产图片扫码入口")
        app.buttons["scan-qr-photo"].tap()
        let photo = app.images.matching(NSPredicate(format: "label BEGINSWITH 'Photo,' OR label BEGINSWITH '照片,'")).firstMatch
        XCTAssertTrue(photo.waitForExistence(timeout: 8), "独立图库必须有真实二维码图片")
        // PhotosPicker 的远程图片 AX 不提供 hittable，点击已定位图片的实际中心。
        photo.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertTrue(app.buttons["连接电脑"].waitForExistence(timeout: 8), "二维码必须经过实际解码并展示电脑身份")
        app.buttons["连接电脑"].tap()
        let done = app.buttons["pairing-done"]
        XCTAssertTrue(done.waitForExistence(timeout: 8), "Mac 授权后必须完成配对，云端失败不能阻断")
        XCTAssertTrue(done.isEnabled, "云端不可用时完成仍须可点击")
        XCTAssertTrue(app.staticTexts["已连接，可以派工"].exists, "控制权限必须明确显示为连接成功")
        XCTAssertFalse(app.buttons["pairing-registration-retry"].exists, "配对页不能要求补做云端注册")
        XCUIDevice.shared.press(.home); app.activate()
        XCTAssertTrue(done.waitForExistence(timeout: 8), "后台恢复不得撤销已完成配对")
        XCTAssertFalse(app.staticTexts["注册已暂停。返回后点击重试完成注册。"].exists, "已配对电脑不能被云端状态标为暂停")
        done.tap()
        XCTAssertTrue(app.staticTexts["直连配对验收 Mac"].waitForExistence(timeout: 8), "完成后电脑记录必须保存")
        let requests = try JSONDecoder().decode([String].self, from: Data(contentsOf: requestsURL)).dropFirst(previousCount)
        XCTAssertTrue(requests.contains("authenticate"), "配对必须取得真实 TLS 授权响应")
        XCTAssertFalse(requests.contains("cloud_registration"), "电脑配对不得发起云端注册")
    }

    /// 配对引导跨两层 Sheet 仍能正常进入，且明确以 Mac 授权完成电脑连接。
    /// 数据构造：真实 App 启动，不预注入云凭据；5秒=启动和系统转场余量，不等待网络。
    /// 执行过程：更多模块→已连接电脑→添加电脑→扫码图片入口→取消→返回电脑页。
    /// 预期结果：出现 Mac 授权说明且页面存活；取消不显示配对成功，不要求云端注册。
    @MainActor func testPairingGuideDoesNotRequireCloudRegistration() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["idea-more-modules"].waitForExistence(timeout: 5), "需要进入真实主页面")
        app.buttons["idea-more-modules"].tap(); app.buttons["已连接电脑"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "电脑列表必须可达")
        app.buttons["add-computer"].tap()
        XCTAssertTrue(app.buttons["scan-qr"].waitForExistence(timeout: 5), "配对页面必须穿过全屏页与Sheet，不能闪退")
        XCTAssertTrue(app.staticTexts["在 Mac 确认授权后，即可连接电脑。"].exists, "配对引导必须以 Mac 授权为完成条件")
        XCTAssertFalse(app.staticTexts["已连接，可以派工"].exists, "未配对不能伪造连接成功")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "single-scan-guide"; shot.lifetime = .keepAlways; add(shot)
        app.buttons["scan-qr"].tap()
        XCTAssertTrue(app.buttons["scan-qr-photo"].waitForExistence(timeout: 5), "无相机的模拟器也能选择真实二维码图片")
        app.buttons["scanner-close"].tap(); app.buttons["pairing-cancel"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "取消必须返回电脑列表")
        XCTAssertFalse(app.buttons["pairing-done"].exists, "取消不能显示已配对完成")
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

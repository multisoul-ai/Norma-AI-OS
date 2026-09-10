import XCTest

/// Keeps XCTest's simulator accessibility bridge alive while Computer Use performs the actual clicks.
/// This harness does not replace Computer Use evidence or inject application data.
final class ComputerUseAcceptanceTests: XCTestCase {
    /// 在显式隔离fixture中保留原生AX桥，供Computer Use点击；结束时继续使用真实已配对App。
    ///
    /// 数据构造：notification-ui现有只读授权，外部ui-cua-requested标记明确启用；不创建新凭据。
    /// 执行过程：启动正式App → 暴露就绪标记 → 等Computer Use操作和结束标记 → 检查App仍在运行。
    /// 预期结果：只在显式模式运行，不能把超时当成功；截图和实际点击结论另存证据，不由本测试宣称。
    @MainActor func testInteractiveSession() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"], path.hasPrefix("/") else {
            throw XCTSkip("仅用于显式Computer Use隔离验收")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-cua-requested").path) else {
            throw XCTSkip("Computer Use未请求本次会话")
        }
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["manage-computers"].waitForExistence(timeout: 5), "正式App须显示电脑管理入口")
        if FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-cua-command").path) {
            app.buttons["workspace-tab-command"].tap()
            XCTAssertTrue(app.staticTexts["优先解决手机连接延迟"].waitForExistence(timeout: 8), "先准备真实计划页，再由Computer Use执行可见操作")
        }
        if FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-direction-flow").path) {
            app.buttons["workspace-tab-command"].tap()
            XCTAssertTrue(app.buttons["adjust-project-direction"].waitForExistence(timeout: 8), "只准备指挥台入口，方向操作由Computer Use执行")
        } else if FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-card-creation").path) {
            app.buttons["workspace-tab-projects"].tap()
            XCTAssertTrue(app.buttons["create-card-toolbar"].waitForExistence(timeout: 8), "只准备创建入口，后续操作由Computer Use执行")
        } else if FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-chat-live").path) {
            app.buttons["workspace-tab-projects"].tap()
            XCTAssertTrue(app.buttons["agent-card-7"].waitForExistence(timeout: 8), "仅准备隔离聊天卡，实际打开和键盘操作由Computer Use执行")
        } else if FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-card-management").path) {
            app.buttons["workspace-tab-projects"].tap()
            XCTAssertTrue(app.buttons["空白验收卡片，卡片操作"].waitForExistence(timeout: 8), "原生空卡应提供管理入口")
        }
        try Data().write(to: directory.appendingPathComponent("ui-cua-ready"), options: .atomic)
        let finished = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-cua-finished").path)
        }, object: nil)
        // 360s = 6min有界Computer Use窗口（包含工具观察和实际点击），仅阻塞测试进程，不阻塞App或通信线程。
        let result = XCTWaiter.wait(for: [finished], timeout: 360)
        XCTAssertEqual(result, .completed, "Computer Use会话必须明确结束，不能将超时当作完成")
        XCTAssertNotEqual(app.state, .notRunning, "实际App不能在点击过程中退出")
    }
}

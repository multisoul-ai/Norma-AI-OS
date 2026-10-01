import XCTest

final class ThreadPlanFlowTests: XCTestCase {
    /// 线程成员选择器可呼叫Norma；未准入失败保留草稿，不创建假消息或任务。
    /// 数据构造：全新Workspace/云授权命名空间，无Mac无云令牌；8秒=转场3秒+本地存储调度5秒。
    /// 执行过程：建空间/线程→选择Norma→输入→发送失败→重启→重新进入线程。
    /// 预期结果：选择器具名，错误可见，草稿逐字保留，无Norma回复或任务卡。
    @MainActor func testNormaMentionFailureKeepsDraft() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = UUID().uuidString
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        app.buttons["workspace-tab-projects"].tap(); app.buttons["create-workspace"].tap()
        app.textFields["workspace-name"].tap(); app.textFields["workspace-name"].typeText("Norma协作")
        app.buttons["workspace-create-save"].tap()
        let workspace = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch
        XCTAssertTrue(workspace.waitForExistence(timeout: 8), "应创建隔离空间")
        workspace.tap(); app.buttons["create-work-thread"].tap()
        app.textFields["thread-title"].tap(); app.textFields["thread-title"].typeText("改进首屏")
        app.buttons["thread-create-save"].tap(); app.buttons["thread-mention"].tap()
        let norma = app.buttons["thread-mention-norma"]
        XCTAssertTrue(norma.waitForExistence(timeout: 8), "没有Coding Agent时也应可以呼叫协调者讨论")
        norma.tap()
        let input = app.textFields["thread-message-input"]
        XCTAssertTrue(input.waitForExistence(timeout: 8), "选择后应返回原线程输入框")
        input.typeText("先讨论首屏")
        XCTAssertEqual(input.value as? String, "@Norma 先讨论首屏", "选择器应插入Norma提及并恢复焦点")
        app.buttons["thread-send"].tap()
        let error = app.staticTexts["thread-action-error"]
        XCTAssertTrue(error.waitForExistence(timeout: 8), "未准入应有明确错误")
        XCTAssertTrue(error.label.contains("访问权限"), "应说明云准入缺失，不能声称Mac拒绝或已执行")
        XCTAssertEqual(input.value as? String, "@Norma 先讨论首屏", "失败不能吞掉输入")
        XCTAssertEqual(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).count, 0, "无回复时不能生成任务")
        attach(app, "P05-norma-authorization-error")
        app.terminate(); app.launch()
        app.buttons["workspace-tab-projects"].tap(); workspace.tap()
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "work-thread-row-")).firstMatch.tap()
        XCTAssertEqual(input.value as? String, "@Norma 先讨论首屏", "失败草稿应跨重启保留")
        XCTAssertEqual(app.otherElements.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-norma-message-")).count, 0, "不能用本地假回复掩盖未准入")
    }

    @MainActor private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}

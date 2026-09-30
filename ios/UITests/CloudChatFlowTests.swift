import XCTest

final class CloudChatFlowTests: XCTestCase {
    /// 没有连接 Mac 或云端邀请时，Norma 单聊页仍能切换文字输入并明确显示受邀状态。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   1 个独立本机测试命名空间；已配对 Mac 数量为 0，云凭据数量为 0。
    ///   8 秒等待 = App 首启约 3 秒 + 本地 SQLite/Keychain 与页面转场余量 5 秒。
    ///
    /// 执行过程：启动 App → 检查菜单没有独立文字模块 → 进入 Norma 主屏 → 点键盘切换文字输入 → 搜索历史 → 返回角色主视觉。
    ///
    /// 预期结果：私聊始终位于 Norma 主屏；未准入时不能发送文字；返回后角色与实时语音入口仍可用。
    @MainActor func testCloudChatEntryDoesNotRequirePairedMac() {
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        app.buttons["idea-more-modules"].tap()
        XCTAssertFalse(app.buttons["norma-text-entry"].exists, "菜单不能出现独立的 Norma 文字模块")
        app.buttons["完成"].tap()
        app.buttons["workspace-tab-command"].tap()
        XCTAssertTrue(app.otherElements["norma-private-screen"].waitForExistence(timeout: 8), "单聊必须留在 Norma 主屏")
        XCTAssertTrue(app.buttons["realtime-call-start"].exists, "Norma 主屏仍须提供实时语音入口")
        let norma = XCTAttachment(screenshot: app.screenshot())
        norma.name = "W07-norma-private-home"
        norma.lifetime = .keepAlways
        add(norma)
        app.buttons["norma-text-toggle"].tap()
        XCTAssertTrue(app.staticTexts["需要邀请"].waitForExistence(timeout: 8), "未受邀时应解释云端准入，而不是要求 Mac 在线")
        XCTAssertFalse(app.buttons["norma-text-send"].isEnabled, "未登记设备绝不能向云服务发消息")
        let textMode = XCTAttachment(screenshot: app.screenshot())
        textMode.name = "W07-inline-text-mode"
        textMode.lifetime = .keepAlways
        add(textMode)
        app.buttons["norma-text-search"].tap()
        XCTAssertTrue(app.textFields["norma-text-search-field"].exists, "本机历史搜索应能从文字页展开")
        app.buttons["norma-text-search"].tap()
        XCTAssertFalse(app.textFields["norma-text-search-field"].exists, "再次点按搜索应收起输入并恢复阅读空间")
        app.buttons["norma-text-toggle"].tap()
        XCTAssertTrue(app.buttons["realtime-call-start"].exists, "切回角色主视觉后语音入口仍须可用")
        XCTAssertFalse(app.buttons["norma-text-back"].exists, "文字输入不能再有独立页面的关闭按钮")
    }
}

import XCTest

final class ChatPhotoFlowTests: XCTestCase {
    /// 已选图片在重启后仍是草稿，单独发送图片经过实际TLS并在对话中恢复可预览缩略图。
    /// 数据构造：独立notification-ui卡7，Computer Use或NotificationFlowTests/testSeedChatPhoto通过正式草稿入口保存的测试JPEG。
    /// 执行过程：重启→原卡→验证纯图片可发→发送→等待准确清空→读取对话图片→打开原图。
    /// 预期结果：仅发送一次，文字不显示本机路径；成功才清理附件，历史图片可再次查看。
    @MainActor func testImageOnlyMessageUploadsAndRendersHistory() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离图片fixture") }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-projects"].tap()
        //8s = 2s心跳+6s真实TLS与原生导航预算；上传与回执最多20s。
        XCTAssertTrue(app.buttons["agent-card-7"].waitForExistence(timeout: 8), "必须进入原隔离卡")
        app.buttons["agent-card-7"].tap()
        let remove = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'chat-remove-photo-'")).firstMatch
        XCTAssertTrue(remove.waitForExistence(timeout: 8), "重启后必须恢复图片草稿")
        XCTAssertTrue(app.buttons["chat-send-message"].isEnabled, "仅有图片时也能发送")
        app.buttons["chat-send-message"].tap()
        XCTAssertTrue(remove.waitForNonExistence(timeout: 20), "准确成功后必须清除已发附件")
        let photo = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'chat-photo-'")).firstMatch
        XCTAssertTrue(photo.waitForExistence(timeout: 8), "历史应显示图片缩略图")
        XCTAssertFalse(app.staticTexts.matching(NSPredicate(format: "label CONTAINS '[[norma-images]]'")).firstMatch.exists, "协议标记不能显示在聊天正文")
        photo.tap()
        XCTAssertTrue(app.images["review-image"].waitForExistence(timeout: 8), "图片须从电脑回读并可打开")
        app.buttons["完成"].tap()
        let records = try JSONSerialization.jsonObject(with: Data(contentsOf: directory.appendingPathComponent("ui-chat-sent.json"))) as? [[String:String]]
        XCTAssertEqual(records?.count, 1, "一次发送只进入同一原卡一次")
        let message = try XCTUnwrap(records?.first?["text"])
        XCTAssertTrue(message.contains(".mobile-images/"), "Agent必须收到已上传文件引用")
        XCTAssertTrue(message.hasSuffix("[[/norma-images]]"), "完整附件不能被消息长度裁剪")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "sent-photo-history"; shot.lifetime = .keepAlways; add(shot)
    }
}

import XCTest

final class RealtimeBackgroundFlowTests: XCTestCase {
    /// 真实App经Home进入系统后台时，既有音频保持采集，返回后仍为同一通话。
    ///
    /// 数据构造：显式回环网关与临时配对；20ms/帧，5s后台窗口理论250帧，仅断言实际帧数增加。
    /// 执行过程：正式入口接通→真实PCM上行→按Home→确认runningBackground→等待5s→
    ///   再次读取服务计数→activate返回→挂断→确认原唯一连接关闭并保存证据。
    /// 预期结果：后台没有挂断或重建连接；返回显示原通话；最后active=0且closed=1。
    /// 层级：真实iOS模拟器App/XCTest系统后台；不代表物理耳机锁屏30分钟或声学延迟达标。
    @MainActor func testAudioContinuesAcrossSystemBackground() async throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "doubao-loopback", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("需要显式回环语音验收")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-native-background").path) else {
            throw XCTSkip("未请求真实系统后台检查")
        }
        let config = try JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path))) as? [String: Any]
        let origin = try XCTUnwrap(config?["origin"] as? String, "应有本次回环网关")
        let url = try XCTUnwrap(URL(string: origin), "有效回环地址")
        XCTAssertEqual(url.host, "127.0.0.1", "现场音频只能进入本机fixture")
        guard url.host == "127.0.0.1" else { return }
        func stats() async throws -> [String: Int] {
            let (data, response) = try await URLSession.shared.data(from: url.appendingPathComponent("stats"))
            XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200, "读取真实回环计数")
            return try JSONDecoder().decode([String: Int].self, from: data)
        }
        func wait(_ predicate: () async throws -> Bool) async throws {
            // 15s = 麦克风就绪3s + 系统调度/连接余量12s；不作为性能阈值。
            for _ in 0..<150 {
                if try await predicate() { return }
                try await Task.sleep(for: .milliseconds(100))
            }
            XCTFail("系统后台语音计数未达到预期状态")
            throw URLError(.timedOut)
        }
        let app = XCUIApplication()
        app.launchArguments += ["-realtime-gateway", origin, "-realtime-provider", "doubao"]
        app.launch()
        defer { app.terminate() }
        // 8s = 场景资源5s + 导航3s，与指挥台入口既有验收一致。
        XCTAssertTrue(app.buttons["workspace-tab-command"].waitForExistence(timeout: 8), "正式指挥台入口必须存在")
        app.buttons["workspace-tab-command"].tap()
        let start = app.buttons["realtime-call-start"], end = app.buttons["realtime-call-end"]
        XCTAssertTrue(start.waitForExistence(timeout: 8), "用户必须主动开聊")
        start.tap()
        try await wait { try await stats()["frames", default: 0] > 0 }
        let before = try await stats()
        XCTAssertTrue(end.exists, "接通后必须能主动挂断")
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5), "必须真正进入系统后台，不能只调用前后台setter")
        try await Task.sleep(for: .seconds(5))
        let background = try await stats()
        XCTAssertEqual(app.state, .runningBackground, "持续音频期间App不能被系统挂起或退出")
        XCTAssertGreaterThan(background["frames", default: 0], before["frames", default: 0], "后台必须继续采集真实PCM")
        XCTAssertEqual(background["sessions"], 1, "退后台不能重建另一通话")
        XCTAssertEqual(background["active"], 1, "原语音连接应继续活动")
        XCTAssertEqual(background["closed"], 0, "进入系统后台不能关闭原媒体连接")
        app.activate()
        XCTAssertTrue(end.waitForExistence(timeout: 8), "返回前台应保留原通话控制")
        XCTAssertFalse(start.exists, "返回不能静默变成重新接通")
        end.tap()
        try await wait { try await stats()["closed"] == 1 }
        let final = try await stats()
        XCTAssertEqual(final["active"], 0, "最终挂断必须释放媒体连接")
        XCTAssertEqual(final["sessions"], 1, "整个检查只能有一通电话")
        XCTAssertEqual(final["invalid_frames"], 0, "后台不能损坏PCM帧格式")
        try JSONSerialization.data(withJSONObject: ["before": before, "background": background, "final": final],
            options: .prettyPrinted).write(to: directory.appendingPathComponent("background-verified.json"))
    }
}

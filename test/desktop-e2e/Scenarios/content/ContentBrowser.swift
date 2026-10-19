import E2EKit
import Fixtures
import Foundation

@MainActor
extension ContentWorkbench {
    private static func selectTab(_ session: Session, label: String, rightOffset: CGFloat) async throws {
        let frame = try await session.capture()
        let text = frame.pick([label], .top)
        // 播放视频时 Vision 可能只返回原生层文字；固定场景的标签位置已逐图核对。
        let point = text.flatMap { $0.center.y < 50 ? $0.center : nil }
            ?? CGPoint(x: frame.windowFrame.width - rightOffset, y: 26)
        try await session.click(local: point, on: frame)
    }

    static func browser(_ session: Session, workspace: String) async throws {
        let server = try ContentBrowserFixture()
        defer { server.stop() }
        try await session.step("浏览器导航与切换会话") {
            let file = try await session.capture()
            // 新建网页的 + 固定在标签栏最右端；固定 1280px 场景中的位置已看图核对。
            let plusX = file.windowFrame.width - 29
            try await session.click(local: CGPoint(x: plusX, y: 26), on: file)
            let blank = try await session.wait(Expectation(allOf: ["about:blank"], timeout: 10))
            guard let address = blank.pick(["about:blank"], .bottom) else { throw E2EError.product("浏览器地址栏不可见") }
            try await session.click(local: address.center, on: blank)
            try await session.shortcut(key: 0, modifiers: [0x37])
            try await session.type(server.address)
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await session.wait(Expectation(allOf: ["BROWSER FIRST E2E", "Session marker"], timeout: 15))
            try await session.click(matching: ["Session marker"], pick: .bottom)
            try await session.type("RETAINED E2E")
            try session.save(try await session.capture(), "09-browser-first.png")
            // 切文件标签再回网页，必须保留页面输入，不能重载替换会话。
            try await selectTab(session, label: "文件", rightOffset: 384)
            _ = try await session.wait(Expectation(anyOf: ["ClipE2E.mp4"] + (0...12).map { String(format: "00:%02d", $0) }, timeout: 8))
            try session.save(try await session.capture(), "08c-video-returned.png")
            try await selectTab(session, label: "127.0.0.1", rightOffset: 280)
            let retained = try await session.wait(Expectation(allOf: ["RETAINED E2E"], timeout: 8))
            try session.save(retained, "10-browser-retained.png")
            try await session.click(matching: ["NEXT PAGE E2E"], pick: .bottom)
            let second = try await session.wait(Expectation(allOf: ["BROWSER SECOND E2E"], timeout: 10))
            try session.save(second, "11-browser-second.png")
            // 导航栏内后退、前进、刷新位置由可见地址栏左侧连续按钮确定。
            guard let location = second.pick(["127.0.0.1"], .bottom) else { throw E2EError.product("当前地址不可见") }
            let toolbarY = location.center.y
            let inputLeft = location.rect.minX
            try await session.click(local: CGPoint(x: inputLeft - 112, y: toolbarY), on: second)
            _ = try await session.wait(Expectation(allOf: ["BROWSER FIRST E2E"], timeout: 8))
            let back = try await session.capture()
            try await session.click(local: CGPoint(x: inputLeft - 75, y: toolbarY), on: back)
            _ = try await session.wait(Expectation(allOf: ["BROWSER SECOND E2E"], timeout: 8))
            try await session.click(local: CGPoint(x: inputLeft - 35, y: toolbarY), on: try await session.capture())
            _ = try await session.wait(Expectation(allOf: ["BROWSER SECOND E2E"], timeout: 8))
            guard try session.contentTabs(workspaceName: workspace).contains(where: { $0.kind == "browser" && $0.resource.hasSuffix("/two") }) else { throw E2EError.product("网页导航位置未持久化") }
            // 右栏拖至新最小宽度附近，验证地址输入和打开入口实际可用。
            let wide = try await session.capture()
            let edge = wide.windowFrame.width * 0.63
            try await session.drag(local: CGPoint(x: edge, y: 260), to: CGPoint(x: wide.windowFrame.width - 190, y: 260), on: wide)
            try await Task.sleep(nanoseconds: 350_000_000)
            let narrow = try await session.capture()
            try session.save(narrow, "14-narrow-browser.png")
            try await session.click(local: CGPoint(x: narrow.windowFrame.width - 120, y: 144), on: narrow)
            try await session.shortcut(key: 0, modifiers: [0x37])
            try await session.type(server.address)
            // 第二行最右侧打开按钮；实际磁盘标签地址与正文继续作硬断言。
            try await session.click(local: CGPoint(x: narrow.windowFrame.width - 30, y: 144), on: try await session.capture())
            _ = try await session.wait(Expectation(anyOf: ["FIRST E2E", "FIRST"], timeout: 8))
            guard try session.contentTabs(workspaceName: workspace).contains(where: { $0.kind == "browser" && $0.resource == server.address }) else { throw E2EError.product("窄栏打开入口未导航") }
            let small = try await session.capture()
            try await session.drag(local: CGPoint(x: small.windowFrame.width - 190, y: 260), to: CGPoint(x: edge, y: 260), on: small)
            _ = try await session.wait(Expectation(allOf: ["BROWSER FIRST E2E"], timeout: 8))
            try await session.click(matching: ["NEXT PAGE E2E"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["BROWSER SECOND E2E"], timeout: 8))
            return "HTTP 页面、会话保留、导航与窄栏地址输入通过；最终地址已写入 SQLite"
        }
        try await session.step("浅色主题下的文件工作台") {
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["交流风格", "Communication Style"], timeout: 10))
            _ = try await SettingsSupport.select(session, labels: ["主题", "Themes"], content: ["Paper Blue", "纸白蓝"])
            try await session.click(matching: ["Paper Blue", "纸白蓝"], pick: .right)
            try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: "paper-blue")
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.wait(Expectation(allOf: ["BROWSER SECOND E2E"], timeout: 8))
            try await selectTab(session, label: "文件", rightOffset: 384)
            _ = try await session.wait(Expectation(anyOf: ["ClipE2E.mp4"] + (0...12).map { String(format: "00:%02d", $0) }, timeout: 8))
            try session.save(try await session.capture(), "12-light-document.png")
            try await showDirectory(session)
            try session.save(try await session.capture(), "13-light-file-tree.png")
            return "真实 Settings 已持久化 Paper Blue，浅色文件正文和目录可见"
        }
    }
}

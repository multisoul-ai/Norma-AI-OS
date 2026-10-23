import AppKit
import E2EKit
import Foundation

/// 完整登录闭环：真实 App ⇄ 本机假提供方的 norma-cloud auth（`dev-auth-server.mjs`）。
/// 不开系统浏览器：驱动从剪贴板取授权链接并代替浏览器跟随重定向，回跳落到 App 的回环监听。
@MainActor
public enum AccountLoginFlow {
    public nonisolated static let name = "account-login-flow"
    static let port = 18793
    static let sessionKey = "account.session"

    public static func run(_ session: Session) async throws {
        let server = try startDevServer(session)
        defer {
            server.terminate()
            session.stop()
        }
        let environment = [
            "NORMA_ACCOUNT_LOGIN": "1",
            "NORMA_E2E_NO_BROWSER": "1",
            "NORMA_CLOUD_URL": "http://127.0.0.1:\(port)",
        ]
        try await session.step("GitHub 授权闭环：复制链接 → 驱动代替浏览器 → 欢迎页") {
            try session.launch(language: "en", environment: environment)
            _ = try await session.wait(Expectation(allOf: ["Sign in to Norma", "Continue with GitHub"], timeout: 25))
            try await session.click(matching: ["Continue with GitHub"], pick: .left)
            _ = try await session.wait(Expectation(allOf: ["Waiting for GitHub authorization", "Copy link"], timeout: 10))
            NSPasteboard.general.clearContents()
            try await session.click(matching: ["Copy link"], pick: .left)
            let link = try await copiedLink()
            guard link.contains("/v1/auth/github/start") else {
                throw E2EError.product("复制的授权链接不符合预期：\(link)")
            }
            // 代替浏览器：跟随 start → 假提供方 → callback → App 回环监听。
            let (_, response) = try await URLSession.shared.data(from: URL(string: link)!)
            guard (response as? HTTPURLResponse)?.statusCode == 200 else {
                throw E2EError.product("回环监听没有向浏览器返回完成页：\(response)")
            }
            let frame = try await session.wait(Expectation(allOf: ["Welcome back, Zhou Kai", "Signed in with GitHub", "Enter Norma"], timeout: 15))
            try session.save(frame, "01-welcome.png")
            return "交换成功，显示欢迎页"
        }
        try await session.step("进入工作台，会话已持久化") {
            try await session.click(matching: ["Enter Norma"], pick: .left)
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "Create your first workspace", "创建工作空间"], absent: ["Welcome back"], timeout: 25))
            try await prove(session) { try session.setting(sessionKey) != nil }
            return "进入工作台，库里存在 \(sessionKey)"
        }
        try await session.step("重启后保持登录，设置页显示资料") {
            session.stop()
            try session.launch(language: "en", environment: environment)
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "Create your first workspace", "创建工作空间"], absent: ["Sign in to Norma"], timeout: 25))
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["Communication Style", "交流风格"], timeout: 10))
            let frame = try await SettingsSupport.select(session, labels: ["Account"], content: ["Zhou Kai"])
            let page = try await session.wait(Expectation(allOf: ["Zhou Kai", "zhoukai@example.com", "Signed in", "GitHub", "Sign out"], timeout: 10))
            try session.save(frame, "02-settings-signed-in-nav.png")
            try session.save(page, "03-settings-signed-in.png")
            return "重启后免登录；账号页显示昵称、邮箱、登录方式与退出登录"
        }
        try await session.step("退出登录清除会话") {
            try await session.click(matching: ["Sign out"], pick: .right)
            let frame = try await session.wait(Expectation(allOf: ["Sign in to Norma", "Continue with GitHub"], absent: ["Zhou Kai"], timeout: 10))
            try session.save(frame, "04-settings-signed-out.png")
            try await prove(session) { try session.setting(sessionKey) == nil }
            return "账号页回到登录卡片，库里会话已清除"
        }
    }

    static func startDevServer(_ session: Session) throws -> Process {
        // appPath = <仓库>/target/debug/norma-os
        let root = URL(fileURLWithPath: session.appPath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["node", root.appendingPathComponent("services/norma-cloud/scripts/dev-auth-server.mjs").path, String(port)]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        Thread.sleep(forTimeInterval: 1.0)
        guard process.isRunning else { throw E2EError.env("本机假 auth 服务启动失败（端口 \(port) 是否被占用、node 是否可用）") }
        return process
    }

    static func copiedLink() async throws -> String {
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            if let text = NSPasteboard.general.string(forType: .string), text.hasPrefix("http") { return text }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("点击「Copy link」后剪贴板没有授权链接")
    }

    static func prove(_ session: Session, _ condition: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(8)
        while Date() < deadline {
            if try condition() { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("等待库状态超时")
    }
}

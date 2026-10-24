import E2EKit
import Foundation

/// 启动登录页与设置 → 账号页。不依赖真实 OAuth：用 `NORMA_E2E_NO_BROWSER` 抑制系统浏览器，
/// `NORMA_CLOUD_URL` 指向无人监听的本机端口，只验证用户可见的状态与持久化。
@MainActor
public enum AccountLoginGate {
    public nonisolated static let name = "account-login-gate"
    static let dismissedKey = "account.login_prompt.dismissed"
    static let environment = [
        "NORMA_ACCOUNT_LOGIN": "1",
        "NORMA_E2E_NO_BROWSER": "1",
        "NORMA_CLOUD_URL": "http://127.0.0.1:9",
    ]

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("首次启动显示登录页，不进入工作台") {
            try session.launch(language: "en", environment: environment)
            let frame = try await session.wait(Expectation(
                allOf: ["Sign in to Norma", "Continue with GitHub", "Continue with Google", "Not now"],
                absent: ["Create Workspace"],
                timeout: 25
            ))
            try session.save(frame, "01-login-page.png")
            return "登录页居中显示两个提供方按钮与「稍后再说」，没有工作台入口"
        }
        try await session.step("点击 GitHub 后进入等待授权，可取消") {
            try await session.click(matching: ["Continue with GitHub"], pick: .left)
            let waiting = try await session.wait(Expectation(
                allOf: ["Waiting for GitHub authorization", "Reopen browser", "Cancel"],
                timeout: 10
            ))
            try session.save(waiting, "02-waiting.png")
            try await session.click(matching: ["Cancel"], pick: .left)
            _ = try await session.wait(Expectation(allOf: ["Continue with GitHub", "Continue with Google"], absent: ["Waiting for GitHub"], timeout: 10))
            return "等待态提供重新打开浏览器与取消；取消后回到选择态"
        }
        try await session.step("稍后再说进入工作台并记住选择") {
            try await session.click(matching: ["Not now"], pick: .left)
            let frame = try await session.wait(Expectation(anyOf: ["Create Workspace", "Create your first workspace", "创建工作空间"], absent: ["Sign in to Norma"], timeout: 25))
            try session.save(frame, "03-workbench.png")
            try await prove(session, key: dismissedKey, value: "1")
            return "进入工作台，库里保存 \(dismissedKey)=1"
        }
        try await session.step("设置里的账号页可随时登录") {
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["Communication Style", "交流风格"], timeout: 10))
            let frame = try await SettingsSupport.select(session, labels: ["Account"], content: ["Sign in to Norma", "Continue with GitHub"])
            try session.save(frame, "04-settings-account.png")
            return "设置侧栏最前有「Account」，页面显示登录卡片与两个提供方按钮"
        }
        try await session.step("重启后直接进入工作台，不再弹登录页") {
            session.stop()
            try session.launch(language: "en", environment: environment)
            let frame = try await session.wait(Expectation(anyOf: ["Create Workspace", "Create your first workspace", "创建工作空间"], absent: ["Sign in to Norma"], timeout: 25))
            try session.save(frame, "05-relaunch-workbench.png")
            return "跳过后重启不再显示登录页"
        }
    }

    static func prove(_ session: Session, key: String, value: String) async throws {
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            if try session.setting(key) == value { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("设置 \(key) 应保存为 \(value)，实际 \(try session.setting(key) ?? "nil")")
    }
}

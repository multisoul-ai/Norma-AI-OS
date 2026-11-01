import E2EKit
import Foundation

/// 冷启动开机动画「Sync」：全窗口视频叠在首个窗口上，播完自动淡出、任意键可跳过。
/// 正式启动时在 Sync / Awaken 两版间随机；e2e 控制下默认不播（避免挡住其他场景），
/// `NORMA_E2E_BOOT_ANIMATION=sync|awaken` 指定版本播放。
/// 判据：动画里的 HUD 文字（NEURAL SYNC / MEMORY LINK / AGENT BUS）经 OCR 可见即在播放；
/// 首页「创建工作空间」可见且 HUD 文字消失即已退场。
@MainActor
public enum BootSyncAnimation {
    public nonisolated static let name = "boot-sync-animation"
    private static let hud = ["NEURAL", "SYNC", "MEMORY LINK", "AGENT BUS", "NORMA CORE"]
    private static let home = ["创建工作空间", "Create Workspace"]

    public static func run(_ session: Session) async throws {
        defer {
            unsetenv("NORMA_E2E_BOOT_ANIMATION")
            session.stop()
        }

        // 放在最前：本步的截图会冷启动 OCR 模型（首张需数秒），之后各步才能准时截到 6 秒内的动画
        try await session.step("未开启（其他 e2e 场景的默认情况）：不播放动画，首页直接可用") {
            unsetenv("NORMA_E2E_BOOT_ANIMATION")
            try session.launch(language: "zh-Hans")
            try await waitForWindow(session)
            let ready = try await session.wait(Expectation(anyOf: home, timeout: 20))
            try session.save(ready, "05-no-boot-default.png")
            guard !hud.contains(where: ready.contains) else {
                throw E2EError.product("未开启时仍出现开机动画，会挡住其他 e2e 场景：\(ready.summary)")
            }
            return ready.summary
        }
        try await session.step("显式开启：冷启动播放神经同步动画，约 6 秒后自动淡出露出首页") {
            session.stop()
            setenv("NORMA_E2E_BOOT_ANIMATION", "sync", 1)
            try session.launch(language: "zh-Hans")
            try await waitForWindow(session)
            let playing = try await session.wait(Expectation(anyOf: hud, timeout: 8))
            try session.save(playing, "01-boot-hud-visible.png")
            guard !home.contains(where: playing.contains) else {
                throw E2EError.product("动画播放中首页文字透出，遮罩未盖住窗口：\(playing.summary)")
            }
            try await Task.sleep(nanoseconds: 2_500_000_000)
            let awake = try await session.capture()
            try session.save(awake, "02-boot-eyes-open.png")
            let after = try await session.wait(Expectation(anyOf: home, absent: hud, timeout: 10))
            try session.save(after, "03-boot-dismissed-home.png")
            return after.summary
        }

        try await session.step("播放中按任意键：动画立即淡出") {
            session.stop()
            try session.launch(language: "zh-Hans")
            try await waitForWindow(session)
            _ = try await session.wait(Expectation(anyOf: hud, timeout: 8))
            let pressed = Date()
            // 49 = Space，首页上无副作用
            try await session.shortcut(key: 49, modifiers: [])
            let skipped = try await session.wait(Expectation(anyOf: home, absent: hud, timeout: 3))
            let elapsed = Date().timeIntervalSince(pressed)
            try session.save(skipped, "04-boot-skipped.png")
            // 跳过淡出 0.25s，加 OCR 截图轮询开销，3s 内必须退场；远早于整段 6s 播完
            guard elapsed < 3 else {
                throw E2EError.product("按键后 \(elapsed)s 才退场，跳过没有生效")
            }
            return "按键后 \(String(format: "%.2f", elapsed))s 退场；\(skipped.summary)"
        }

        try await session.step("Awaken 版：暗场光束横过她闭着的双眼后睁眼，首页被完全遮住，约 9 秒后退场") {
            session.stop()
            setenv("NORMA_E2E_BOOT_ANIMATION", "awaken", 1)
            try session.launch(language: "zh-Hans")
            try await waitForWindow(session)
            try await Task.sleep(nanoseconds: 2_000_000_000)
            let covered = try await session.capture()
            try session.save(covered, "06-awaken-eyes.png")
            // Awaken 只有一行极小的光束读数，不作为 OCR 判据；判据是首页被遮住、之后露出
            guard !home.contains(where: covered.contains) else {
                throw E2EError.product("Awaken 播放中首页文字透出，遮罩未盖住窗口：\(covered.summary)")
            }
            let after = try await session.wait(Expectation(anyOf: home, timeout: 10))
            try session.save(after, "07-awaken-dismissed-home.png")
            return after.summary
        }

    }

    /// 动画从首窗口的第一帧开始，场景必须在窗口刚出现时就截图；此前 App 已开放控制通道但
    /// 还没有窗口，截图会报「window unavailable」。只吞这一种启动竞态，其他错误照常抛出。
    private static func waitForWindow(_ session: Session) async throws {
        let deadline = Date().addingTimeInterval(15)
        while true {
            do {
                _ = try await session.capture()
                return
            } catch let error as E2EError where "\(error)".contains("window unavailable") && Date() < deadline {
                try await Task.sleep(nanoseconds: 100_000_000)
            }
        }
    }
}

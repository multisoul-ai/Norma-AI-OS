import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 真实 Codex 工具与原生 turn 生命周期；模型服务及幕僚响应采用本机可控回放。
/// 每个等待阶段核对正向运行态、负向完成通知，并保存连续帧供边缘动画验收。
@MainActor
public enum AgentCodexWaitCompletion {
    public nonisolated static let name = "agent-codex-wait-completion"

    public static func run(_ session: Session) async throws {
        var fixture: CodexWaitFixture?
        var project: URL?
        defer {
            session.stop()
            fixture?.stop()
            if let project { try? FileManager.default.removeItem(at: project) }
        }
        let workspace = "CodexWaitCheck"
        try await session.step("启动隔离真实 App，创建 Codex 验收项目") {
            guard let output = session.output else { throw E2EError.env("缺少证据目录") }
            let hook = URL(fileURLWithPath: session.appPath).deletingLastPathComponent().appendingPathComponent("multisoul-agent-hook")
            let install = try await CodexWaitFixture(root: output.appendingPathComponent("fixture"), hookBinary: hook.path)
            fixture = install
            try session.launch(homeDirectory: install.home)
            try await session.resize(width: 1280, height: 800)
            try await BrainSupport.createWorkspace(session, name: workspace)
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            let root = try CanvasProjectFixture.install(dataRoot: data, workspaceName: workspace)
            project = root
            try install.trust(project: root)
            try Catalog.suppressFeatureGuide(database: data.appendingPathComponent("app.sqlite"))
            try session.launch(homeDirectory: install.home)
            try await session.resize(width: 1280, height: 800)
            // 直接按空间名进入，避免依赖其他任务尚未提交的导航修改。
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            try await session.click(matching: [workspace], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            try await BrainSupport.dismissGuide(session)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            return try await session.capture().summary
        }
        guard let fixture else { throw E2EError.env("Codex 夹具未建立") }

        try await session.step("启动安装的真实 Codex 并提交当前回合") {
            let frame = try await session.capture()
            try await session.click(local: CGPoint(x: frame.windowFrame.width / 2, y: frame.windowFrame.height - 38), on: frame)
            try await session.type("/new codex")
            try await session.shortcut(key: 36, modifiers: [])
            let startup = try await session.wait(Expectation(anyOf: ["gpt-5.5", "Wait Test", "Use existing model"], timeout: 40))
            if startup.contains("Use existing model") {
                guard let menu = startup.pick(["Use existing model"], .top) else { throw E2EError.product("模型介绍菜单不可定位") }
                try await session.click(local: menu.center, on: startup)
                try await session.shortcut(key: 125, modifiers: [])
                try await session.shortcut(key: 36, modifiers: [])
            }
            let ready = try await session.wait(Expectation(anyOf: ["gpt-5.5", "Wait Test"], absent: ["Use existing model"], timeout: 30))
            try session.save(ready, "00-codex-ready.png")
            guard let model = ready.pick(["gpt-5.5", "Wait Test"], .top) else { throw E2EError.product("真实 Codex 模型栏不可定位") }
            try await session.click(local: model.center, on: ready)
            try await session.type("Run the terminal check, wait for the background process, then summarize the result.")
            try await session.shortcut(key: 36, modifiers: [])
            try await fixture.waitPhase("api_wait")
            return "真实 Responses 请求已到达本机，尚未返回模型正文"
        }

        try await session.step("等待 API 响应期间持续运行，幕僚未收到完成") {
            try await running(session, fixture: fixture, prefix: "01-api-wait")
        }
        try fixture.release("release-api")
        try await fixture.waitPhase("thinking")
        try await session.step("模型思考期间持续运行，幕僚未收到完成") {
            try await running(session, fixture: fixture, prefix: "02-thinking")
        }
        try fixture.release("release-thinking")
        try await fixture.waitPhase("terminal_wait")
        try await session.step("真实 Waiting for background terminal 保持运行") {
            let wait = try await session.wait(Expectation(allOf: ["Waiting for background terminal", "运行中"], absent: ["Done"], timeout: 25))
            try session.save(wait, "03-background-terminal-banner.png")
            return try await running(session, fixture: fixture, prefix: "03-background-terminal")
        }
        try await session.step("原生回合仍活跃时注入提前 Stop，仍运行且不通知幕僚") {
            try fixture.release("inject-stop")
            try await fixture.waitFile("injected.json")
            let data = try Data(contentsOf: fixture.root.appendingPathComponent("injected.json"))
            let receipt = try JSONSerialization.jsonObject(with: data) as? [String: Any]
            guard receipt?["status"] as? Int == 200 else { throw E2EError.product("提前 Stop 未送达生产 Hook endpoint") }
            try await Task.sleep(nanoseconds: 2_500_000_000)
            return try await running(session, fixture: fixture, prefix: "04-premature-stop-rejected")
        }
        try fixture.release("finish-terminal")
        try await fixture.waitPhase("final_api_wait", timeout: 130)
        try await session.step("后台终端已结束，当前回合等待最终 API 时仍运行") {
            try await running(session, fixture: fixture, prefix: "05-final-api-wait")
        }
        try fixture.release("release-final")
        try await session.step("匹配的原生回合结束后才 Done，并向幕僚通知一次") {
            let done = try await session.wait(Expectation(allOf: ["CODEX_WAIT_COMPLETE", "Done"], timeout: 35))
            try session.save(done, "06-native-turn-completed.png")
            try requireCardState(done, running: false)
            try await fixture.waitFile("brain-finished.jsonl", timeout: 30)
            try await Task.sleep(nanoseconds: 3_000_000_000)
            let finished = fixture.finished()
            guard finished.count == 1, finished[0].contains("CODEX_WAIT_COMPLETE"), !finished[0].contains("PREMATURE_STOP_SENTINEL") else {
                throw E2EError.product("幕僚完成通知须恰好一次且为最终回复，实际：\(finished)")
            }
            try await samples(session, prefix: "06-done")
            return "Done；真实幕僚输入链路收到一次最终回复，未收到提前 Stop 的结果"
        }
        try await session.step("真实幕僚活动显示对应完成处理结果") {
            _ = try await BrainActivity.openPanel(session)
            let activity = try await session.wait(Expectation(allOf: ["CODEX_BRAIN_COMPLETION_RECEIVED"], timeout: 15))
            try session.save(activity, "07-brain-completion.png")
            guard fixture.finished().count == 1 else { throw E2EError.product("幕僚重复收到完成通知") }
            return activity.summary
        }
    }

    private static func running(_ session: Session, fixture: CodexWaitFixture, prefix: String) async throws -> String {
        let frame = try await session.wait(Expectation(allOf: ["运行中"], absent: ["Done"], timeout: 12))
        try session.save(frame, prefix + ".png")
        try requireCardState(frame, running: true)
        guard fixture.finished().isEmpty else { throw E2EError.product("等待期间幕僚提前收到完成：\(fixture.finished())") }
        try await samples(session, prefix: prefix)
        guard fixture.finished().isEmpty else { throw E2EError.product("连续等待期间幕僚提前收到完成") }
        return frame.summary + "；幕僚完成通知=0；保存4帧供运行动画比较"
    }

    private static func requireCardState(_ frame: Frame, running: Bool) throws {
        guard let title = frame.pick(["Alan"], .top) else { throw E2EError.product("未识别到 Alan 卡片标题") }
        let header = frame.texts.filter { abs($0.center.y - title.center.y) < 16 }.map(\.value).joined(separator: " | ")
        guard header.contains(running ? "运行中" : "Done"), !header.contains(running ? "Done" : "运行中") else {
            throw E2EError.product("卡片标题状态不符（不使用全局计数作为运行证据）：\(header)")
        }
    }

    private static func samples(_ session: Session, prefix: String) async throws {
        for index in 0..<4 {
            if index > 0 { try await Task.sleep(nanoseconds: 450_000_000) }
            try session.saveSnapshot(try await session.snapshot(), "\(prefix)-motion-\(index).png")
        }
    }
}

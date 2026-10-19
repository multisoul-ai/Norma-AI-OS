import E2EKit
import Fixtures
import Foundation

@MainActor
public enum VoiceLiveCall {
    public nonisolated static let name = "voice-live-call"

    public static func run(_ session: Session) async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_LIVE_TEST_KEY_FILE"] else {
            throw E2EError.env("真实云端场景需要NORMA_LIVE_TEST_KEY_FILE；不会读取用户现有Key")
        }
        defer {
            session.stop()
            // Remove the entire isolated credential store, including SQLite sidecar files.
            if let root = session.dataRoot { try? FileManager.default.removeItem(at: root) }
        }
        try await session.step("准备私有隔离凭据并打开真实语音设置") {
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            session.stop()
            try LiveCredentialFixture.install(database: session.databaseURL(), keyFile: URL(fileURLWithPath: path))
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["个性", "Persona", "语音", "Voice"], timeout: 8))
            try await session.click(matching: ["语音", "Voice"], pick: .left)
            _ = try await session.wait(Expectation(allOf: ["GPT-Live"], timeout: 8))
            return "独立GPT模式及凭据已加载；测试后删除整份隔离数据"
        }
        try await session.step("真实连接并启用麦克风") {
            try await clickButton(session, ["连接", "Connect"])
            do {
                let connected = try await session.wait(Expectation(anyOf: ["通话中", "In call"], timeout: 25))
                try session.save(connected, "01-live-connected.png")
            } catch {
                let frame = try await session.capture()
                try session.save(frame, "01-live-connect-failed.png")
                if frame.summary.localizedCaseInsensitiveContains("permission") || frame.summary.contains("权限") || frame.summary.contains("授权") {
                    throw E2EError.env("真实麦克风授权尚未确认，请在系统提示中允许测试Norma使用麦克风。\(frame.summary)")
                }
                throw error
            }
            return "真实App已进入通话状态，采集资源Ready；声音和声学指标另验"
        }
        try await session.step("真实静音恢复并挂断核对用量") {
            try await clickButton(session, ["静音", "Mute"])
            _ = try await session.wait(Expectation(anyOf: ["取消静音", "Unmute"], timeout: 8))
            try session.save(try await session.capture(), "02-live-muted.png")
            try await clickButton(session, ["取消静音", "Unmute"])
            _ = try await session.wait(Expectation(anyOf: ["静音", "Mute"], timeout: 8))
            try await clickButton(session, ["挂断", "End call"])
            try await session.scroll(pixels: -1000)
            _ = try await session.wait(Expectation(anyOf: ["诊断", "Diagnostics"], timeout: 5))
            try await session.click(matching: ["诊断", "Diagnostics"], pick: .bottom)
            try await session.scroll(pixels: -300)
            let closed = try await session.wait(Expectation(anyOf: ["最终用量", "Final usage"], absent: ["通话中", "In call"], timeout: 15))
            try session.save(closed, "03-live-closed.png")
            if let output = session.output {
                let log = try String(contentsOf: output.appendingPathComponent("app.log"), encoding: .utf8)
                let released = log.components(separatedBy: .newlines).filter { $0.contains("GPT-Live call resources released") }
                guard released.count == 1, released[0].contains("error=\"\"") else {
                    throw E2EError.product("正常挂断必须释放资源且不报会话错误：\(released.joined(separator: "; "))")
                }
            }
            return "静音及恢复状态经过真实窗口；挂断后显示最终云端用量"
        }
    }

    static func clickButton(_ session: Session, _ labels: [String]) async throws {
        let frame = try await session.capture()
        guard let button = labels.compactMap({ frame.exact($0) }).first ?? frame.pick(labels, .bottom) else {
            throw E2EError.product("未发现独立按钮：\(labels.joined(separator: "/"))；\(frame.summary)")
        }
        try await session.click(local: button.center, on: frame)
    }
}

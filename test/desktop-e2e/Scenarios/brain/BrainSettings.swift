import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 设置 → Norma 大脑：页面内容、开关写入配置、策略选择写入配置。
@MainActor
public enum BrainSettings {
    public nonisolated static let name = "brain-settings"

    public static func run(_ session: Session) async throws {
        var brainDir: URL?
        defer {
            session.stop()
            if let brainDir { try? FileManager.default.removeItem(at: brainDir) }
        }
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-brain-settings-e2e-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        brainDir = root
        let config = root.appendingPathComponent("config.json")

        try await session.step("打开设置，进入 Norma 大脑") {
            setenv("NORMA_BRAIN_DIR", root.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "en", 1)
            try await SettingsSupport.open(session)
            let frame = try await SettingsSupport.select(
                session, labels: ["Norma Brain"], content: ["Enable background brain"])
            try session.save(frame, "01-brain-page.png")
            for needle in ["Backend", "Strategy", "Speed first", "Quality first"] {
                guard frame.contains(needle) else {
                    throw E2EError.product("页面缺少「\(needle)」：\(frame.summary)")
                }
            }
            return "页面显示开关、后端与策略"
        }

        try await session.step("点开关，配置写入 enabled=true") {
            let frame = try await session.capture()
            guard let title = frame.pick(["Enable background brain"], .bottom) else {
                throw E2EError.product("找不到开关标题：\(frame.summary)")
            }
            // 开关在内容卡片右端，垂直居中于卡片，比标题行低约窗口高度的 2.2%。
            try await session.click(
                local: CGPoint(
                    x: Double(frame.windowFrame.width) * 0.805,
                    y: title.center.y + Double(frame.windowFrame.height) * 0.022),
                on: frame)
            try await waitConfig(config, contains: "\"enabled\": true")
            try session.save(try await session.capture(), "02-brain-enabled.png")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }

        try await session.step("选择“速度优先”，配置写入 strategy=speed") {
            try await session.click(matching: ["Speed first"], pick: .bottom)
            try await waitConfig(config, contains: "\"strategy\": \"speed\"")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }
    }

    private static func waitConfig(_ url: URL, contains needle: String) async throws {
        let deadline = Date().addingTimeInterval(6)
        while Date() < deadline {
            if let text = try? String(contentsOf: url, encoding: .utf8), text.contains(needle) { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        let now = (try? String(contentsOf: url, encoding: .utf8)) ?? "(没有文件)"
        throw E2EError.product("配置里 6 秒内没有出现 \(needle)，当前：\(now)")
    }
}

import E2EKit
import Foundation

/// “编排”设置页的文件路径输出约定开关：默认开启，点击后落库为 false，再点击恢复 true。
@MainActor
public enum SettingsOrchestrationPathConvention {
    public nonisolated static let name = "settings-orchestration-path-convention"
    private static let key = "orchestration.path_convention.v1"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("打开编排页并看到开关行") {
            try await SettingsSupport.open(session)
            let frame = try await SettingsSupport.select(
                session, labels: ["编排", "Orchestration"], content: ["文件路径输出约定", "File path output convention"])
            try session.save(frame, "01-orchestration-default.png")
            guard try session.setting(key) == nil else { throw E2EError.product("默认状态不应已有保存值") }
            return "编排页顶部出现开关行，默认未写库（视为开启）"
        }
        try await session.step("关闭开关并落库") {
            try await toggle(session)
            try session.save(try await session.capture(), "02-off.png")
            try await SettingsSupport.prove(session, key: key, value: "false")
            return "点击后设置保存为 false"
        }
        try await session.step("再次开启") {
            try await toggle(session)
            try session.save(try await session.capture(), "03-on.png")
            try await SettingsSupport.prove(session, key: key, value: "true")
            return "再次点击后设置保存为 true"
        }
    }

    /// 设置弹窗居中；开关位于标题行右端，距窗口右缘约 239px，在标题下方约 16px（行内垂直居中，已看图核对）。
    private static func toggle(_ session: Session) async throws {
        let frame = try await session.capture()
        guard let title = frame.pick(["文件路径输出约定", "File path output convention"], .top) else {
            throw E2EError.product("找不到开关行标题：\(frame.summary)")
        }
        try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) - 239, y: title.center.y + 16), on: frame)
    }
}

import E2EKit
import Fixtures
import Foundation

/// 真实App启动迁移旧幕僚数据，设置读写使用全局目录，重启不覆盖新设置或旧备份。
@MainActor
public enum IslandMigration {
    public nonisolated static let name = "island-migration"

    public static func run(_ session: Session) async throws {
        let root = URL(fileURLWithPath: "/private/tmp/norma-island-migration-\(UUID().uuidString)")
        defer { session.stop(); try? FileManager.default.removeItem(at: root) }
        setenv("NORMA_BRAIN_DIR", root.appendingPathComponent("bootstrap").path, 1)
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
        session.stop()
        let fixture = try IslandMigrationFixture.seed(database: database, dataRoot: dataRoot, base: root.appendingPathComponent("legacy"))
        setenv("NORMA_BRAIN_DIR", fixture.base.path, 1)
        try await session.step("真实启动迁移两个旧项目，完整保留数据与备份") {
            try session.launch(language: "zh-Hans")
            try await session.resize(width: 1280, height: 840)
            _ = try await session.visualCheck([VisualCriterion(id: "launched", text: "首页显示Migration Alpha和Migration Beta两个工作空间，没有迁移失败提示")])
            try await waitForCurrent(fixture)
            try fixture.verify(speaking: false)
            return "两个目标、两条提议、四条决策日志和三条确认偏好完整；所有旧文件字节保持"
        }
        try await session.step("设置页读取迁移后的静音偏好") {
            try await openSettings(session, english: false)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "muted", text: "幕僚设置页可见，幕僚总开关关闭，语音汇报开关关闭；界面为中文，没有数据准备失败提示")
            ]), "01-migrated-muted.png")
            return "实际界面读取旧配置"
        }
        try await session.step("修改语音开关只写全局目录") {
            try await session.visualClick("点击语音汇报这一行右侧关闭的开关，使其开启；不要点幕僚总开关")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "speaking", text: "语音汇报开关已开启，幕僚总开关仍关闭")
            ]), "02-global-setting.png")
            let deadline = Date().addingTimeInterval(5)
            while Date() < deadline {
                if try fixture.object(fixture.current.appendingPathComponent("config.json"))["speak"] as? Bool == true { break }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            try fixture.verify(speaking: true)
            return "新偏好已保存到global/current，旧配置仍原样保留"
        }
        try await session.step("英文重启后新偏好保留，不重复导入旧数据") {
            let target = try FileManager.default.destinationOfSymbolicLink(atPath: fixture.current.path)
            session.stop()
            try session.launch(language: "en")
            try await session.resize(width: 1280, height: 840)
            try await openSettings(session, english: true)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "restored", text: "英文Chief of Staff设置页可见，Voice reports开启，总开关关闭；没有中文残留或迁移失败提示")
            ]), "03-restarted-en.png")
            try fixture.verify(speaking: true)
            guard try FileManager.default.destinationOfSymbolicLink(atPath: fixture.current.path) == target else {
                throw E2EError.product("重启重新导入了旧备份")
            }
            if let output = session.output {
                try FileManager.default.copyItem(at: fixture.base, to: output.appendingPathComponent("brain-state"))
            }
            return "同一代数据被复用，目标/提议/日志/确认偏好完整且备份不变"
        }
    }

    private static func waitForCurrent(_ fixture: IslandMigrationFixture) async throws {
        let deadline = Date().addingTimeInterval(10)
        while Date() < deadline && !FileManager.default.fileExists(atPath: fixture.current.appendingPathComponent("config.json").path) {
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        guard FileManager.default.fileExists(atPath: fixture.current.appendingPathComponent("config.json").path) else {
            throw E2EError.product("真实启动未完成全局迁移")
        }
    }

    private static func openSettings(_ session: Session, english: Bool) async throws {
        guard let main = try await session.nativeWindows().first(where: { $0.width > 900 }) else {
            throw E2EError.product("缺少主窗口")
        }
        try await session.selectNativeWindow(main.number)
        try await session.shortcut(key: 43, modifiers: [0x37])
        try await session.visualClick(english ? "点击设置左侧Chief of Staff菜单项" : "点击设置左侧幕僚菜单项")
    }
}

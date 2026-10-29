import E2EKit
import Fixtures
import Foundation
import Testing

struct HomeDesignFixtureTests {
    /// 场景：同一隔离库依次验收深色12空间、浅色12空间、单空间。
    /// 构造：保留生产配置版本表的外键、主键和空间创建触发器，首次安装会产生版本行。
    /// 执行：连续三次安装首页夹具；模拟E2E在主题/数量切换时复用同一数据库。
    /// 预期：重建不触发重复主键，版本行随空间级联清除，最后没有旧空间或旧项目残留。
    @Test func repeatedHomeFixtureInstallClearsWorkspaceDependents() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let database = root.appendingPathComponent("app.sqlite")
        try Data().write(to: database)
        let schema = [
            "CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at_unix INTEGER NOT NULL)",
            "CREATE TABLE desktop_workspaces(id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at_unix_ms INTEGER NOT NULL, last_opened_at_unix_ms INTEGER NOT NULL)",
            "CREATE TABLE desktop_project_areas(id TEXT PRIMARY KEY, desktop_workspace_id TEXT NOT NULL REFERENCES desktop_workspaces(id) ON DELETE CASCADE, project_key TEXT NOT NULL, root_path TEXT NOT NULL, display_name TEXT NOT NULL, added_at_unix_ms INTEGER NOT NULL)",
            "CREATE TABLE desktop_configuration_revisions(workspace_id TEXT PRIMARY KEY REFERENCES desktop_workspaces(id) ON DELETE CASCADE, revision INTEGER NOT NULL)",
            "CREATE TRIGGER desktop_chat_workspace_created AFTER INSERT ON desktop_workspaces BEGIN INSERT INTO desktop_configuration_revisions VALUES(NEW.id,1); END",
        ]
        try SQLRaw.execute(database: database, statements: schema.map { ($0, []) })
        try HomeDesignFixture.install(database: database, count: 12, theme: "sapphire-night", alternate: false)
        #expect(try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM desktop_configuration_revisions") == 12,
                "第一次安装必须真实创建12条配置版本记录，确保重建覆盖外键关联")
        try HomeDesignFixture.install(database: database, count: 12, theme: "paper-blue", alternate: true)
        #expect(try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM desktop_workspaces") == 12,
                "浅色重建应保留12个空间，不累加或漏建")
        try HomeDesignFixture.install(database: database, count: 1, theme: "paper-blue", alternate: true)
        #expect(try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM desktop_project_areas") == 1,
                "单空间夹具只保留第一空间的一个项目")
        #expect(try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM desktop_configuration_revisions") == 1,
                "单空间重建后只允许一条对应的配置版本记录")
        #expect(try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM desktop_configuration_revisions WHERE workspace_id != 'home-0'") == 0,
                "旧空间的关联配置版本必须全部移除")
    }
}

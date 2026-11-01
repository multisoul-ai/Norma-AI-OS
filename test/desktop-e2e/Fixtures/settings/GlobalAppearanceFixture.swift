import E2EKit
import Foundation

public enum GlobalAppearanceFixture {
    /// 仅在隔离 App 停止时安装旧版空间/项目覆盖。展开项目优先，原全局值不同，防止迁移假阳性。
    public static func install(dataRoot: URL) throws -> URL {
        let db = dataRoot.appendingPathComponent("app.sqlite")
        try Catalog.seedWorkspace(database: db, name: "Legacy Appearance")
        let project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Legacy Appearance")
        try Catalog.suppressFeatureGuide(database: db)
        try Catalog.seedSetting(database: db, key: "general.language.v1", value: "en")
        try Catalog.seedSetting(database: db, key: "wallpaper.kind", value: "builtin")
        try Catalog.seedSetting(database: db, key: "wallpaper.value", value: "wallpapers/norma-golden-orbit.png")
        try SQLRaw.execute(database: db, statements: [
            ("UPDATE desktop_project_areas SET wallpaper_json=?, dispatch_effect_json=?", [
                "{\"kind\":\"builtin\",\"rel\":\"wallpapers/norma-violet-nebula.png\"}",
                "{\"style\":\"fireworks\",\"voice\":\"lightning\"}",
            ]),
            ("UPDATE desktop_workspaces SET expanded_area_id=(SELECT id FROM desktop_project_areas LIMIT 1), wallpaper_json=?, dispatch_effect_json=?", [
                "{\"kind\":\"builtin\",\"rel\":\"wallpapers/norma-blue-horizon.png\"}", "{\"style\":\"meteor\"}",
            ]),
            ("DELETE FROM settings WHERE key IN ('appearance.global_migrated.v1','effects.dispatch.v1','wallpaper.global_candidates.v1')", []),
        ])
        return project
    }
}

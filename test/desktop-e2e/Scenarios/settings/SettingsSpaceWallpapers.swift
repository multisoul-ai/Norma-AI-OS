import E2EKit

/// 场景：五款内置太空壁纸在Ink Black 与 Graphite 主题下可预览、选择，并在重启后恢复。
/// 构造：空隔离数据库，五条交付资源路径；逐项由真实设置页选择，读取 SQLite 核对。
/// 预期：每次保存正确资源，中文名称与Graphite 预览可见，重启保留最后一款且不要求下载。
@MainActor
public enum SettingsSpaceWallpapers {
    public nonisolated static let name = "settings-space-wallpapers"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        let wallpapers = [
            ("Blue Horizon", "blue-horizon"),
            ("Golden Orbit", "golden-orbit"),
            ("Violet Nebula", "violet-nebula"),
            ("Emerald Aurora", "emerald-aurora"),
            ("Silver Moonscape", "silver-moonscape"),
        ]
        try await session.step("打开内置太空壁纸库") {
            try await SettingsSupport.open(session)
            try await SettingsSupport.language(session, label: "English", saved: "en")
            _ = try await SettingsSupport.select(
                session, labels: ["Wallpaper"], content: ["Wallpaper Library"]
            )
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
            let page = try await session.wait(Expectation(allOf: ["Applies globally"], absent: ["Follow workspace", "Directory background"]))
            try session.save(page, "00-global-wallpaper.png")
            return "全局壁纸，页面不再有范围切换或跟随开关"
        }
        for (index, wallpaper) in wallpapers.enumerated() {
            try await session.step("选择 \(wallpaper.0)") {
                try await selectWallpaper(session, name: wallpaper.0)
                try await proveWallpaper(session, slug: wallpaper.1)
                try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 1800)
                // 窄内容区的当前卡纵向排列，轻滚使名称与保存状态同时进入视口。
                try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: -100)
                let current = try await session.wait(Expectation(allOf: [wallpaper.0, "Saved"]))
                try session.save(current, "0\(index + 1)-\(wallpaper.1)-dark.png")
                return "SQLite wallpaper.value=wallpapers/norma-\(wallpaper.1).png；静态原图无需下载"
            }
        }
        try await session.step("Graphite 主题与中文素材库") {
            _ = try await SettingsSupport.select(session, labels: ["Themes"], content: ["Graphite"])
            try await session.click(matching: ["Graphite"], pick: .right)
            try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: "graphite")
            try await SettingsSupport.language(session, label: "中文", saved: "zh-CN")
            _ = try await SettingsSupport.select(session, labels: ["壁纸"], content: ["壁纸素材库"])
            try await selectWallpaper(session, name: "蔚蓝地平线")
            try await proveWallpaper(session, slug: "blue-horizon")
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 1800)
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: -540)
            // Vision 偶尔混淆「鎏」和「白」；用唯一的「金星环」「月境」定位，完整名称由看图确认。
            let library = try await session.wait(Expectation(anyOf: ["金星环"], allOf: [
                "蔚蓝地平线", "紫色星云", "翡翠极光", "月境",
            ]))
            try session.save(library, "06-space-library-graphite.png")
            return "Graphite 下五款中文名称和缩略图可见，蔚蓝地平线已保存"
        }
        try await session.step("重启恢复太空壁纸") {
            session.stop()
            try await SettingsSupport.open(session)
            _ = try await SettingsSupport.select(session, labels: ["壁纸"], content: ["壁纸素材库"])
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 1800)
            try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: -100)
            try await proveWallpaper(session, slug: "blue-horizon")
            let current = try await session.wait(Expectation(allOf: ["蔚蓝地平线"]))
            try session.save(current, "07-restored-space-wallpaper.png")
            return "重启保留内置选择、中文与Graphite 主题"
        }
    }

    private static func selectWallpaper(_ session: Session, name: String) async throws {
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 1800)
        for _ in 0..<10 {
            let frame = try await session.capture()
            if let tile = frame.pick([name], .bottom) {
                try await session.click(local: tile.center, on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -150)
        }
        throw E2EError.product("内置壁纸库找不到 \(name)")
    }

    private static func proveWallpaper(_ session: Session, slug: String) async throws {
        try await SettingsSupport.prove(session, key: "wallpaper.kind", value: "builtin")
        try await SettingsSupport.prove(session, key: "wallpaper.value", value: "wallpapers/norma-\(slug).png")
    }
}

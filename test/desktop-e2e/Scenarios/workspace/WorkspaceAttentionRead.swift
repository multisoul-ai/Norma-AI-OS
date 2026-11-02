import E2EKit
import Fixtures
import Foundation

/// Apple 即时反馈：打开成果后移除对应提醒，键盘与鼠标共用行为；不增加动效。
@MainActor
public enum WorkspaceAttentionRead {
    public nonisolated static let name = "workspace-attention-read"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        let name = "AttentionRead"
        var database: URL?
        var root: URL?
        try await session.step("建立含真实项目的隔离工作空间") {
            try session.launch(language: "zh-Hans")
            let db = try session.databaseURL()
            database = db
            _ = try await session.waitCatalog(timeout: 20) { _ in
                FileManager.default.fileExists(atPath: db.path)
            }
            session.stop()
            try Catalog.seedWorkspace(database: db, name: name)
            try Catalog.suppressFeatureGuide(database: db)
            guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            let project = dataRoot.appendingPathComponent("attention-project")
            root = project
            try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
            try ContentFixture.git(project, ["init"])
            try Catalog.seedArea(database: db, workspaceName: name, root: project, displayName: "Review Project")
            return try session.prove(names: [name], areaCount: 1).summary
        }
        guard let database, let root else { throw E2EError.env("缺少夹具路径") }
        // 旧 paper-blue 偏好会在生产启动中迁移为 Ink Black，实际验收两个现行深色主题。
        for (language, theme) in [("zh-Hans", "sapphire-night"), ("en", "paper-blue")] {
            try await session.step("\(theme)：新版本成果重新出现") {
                session.stop()
                try AttentionFixture.seed(database: database, root: root, cycle: language)
                // gamma 已提交、没有当前 diff，成果打开必须回退到该回合的冻结正文。
                try ContentFixture.git(root, ["add", "gamma.txt"])
                try ContentFixture.git(root, ["-c", "user.name=Norma E2E", "-c", "user.email=e2e@example.invalid", "commit", "-m", "Freeze gamma"])
                try VoiceConfigurationFixture.theme(database: database, id: theme)
                try session.launch(language: language)
                try await session.resize(width: 1280, height: 800)
                try await enterOverview(session)
                let frame = try await session.visualCheck([
                    VisualCriterion(id: "unread", text: "左栏需要你处理 / Needs attention 计数为 5：两条回合成果和 alpha.txt、beta.txt、gamma.txt 三条文件；运行中和待开始分区保留。若尚在加载则等待")
                ])
                try session.save(frame, "\(theme)-01-unread.png")
                return "2 个成果 + 3 个文件 = 5 条提醒；同路径新版本仍提醒"
            }
            try await session.step("打开 alpha 文件仅清除这一条提醒") {
                try await session.visualClick("点击左栏 File ready for review / 文件待查看、文件名为 alpha.txt 的行，或该行右侧 Open / 打开")
                let frame = try await session.visualCheck([
                    VisualCriterion(id: "opened", text: "右侧已显示 alpha.txt 的冻结正文 Frozen alpha.txt，不能是加载错误或空标签"),
                    VisualCriterion(id: "remaining", text: "左栏待处理计数为 4，alpha.txt 文件提醒消失，beta.txt、gamma.txt 和两条成果提醒仍在")
                ])
                try session.save(frame, "\(theme)-02-file-read.png")
                return "正文已打开，另一文件保持未读"
            }
            try await session.step("打开 beta 后该回合成果也清除") {
                try await session.visualClick("点击左栏 beta.txt 的文件提醒，打开 beta.txt")
                let frame = try await session.visualCheck([
                    VisualCriterion(id: "opened", text: "右侧显示 beta.txt 的冻结正文 Frozen beta.txt"),
                    VisualCriterion(id: "remaining", text: "左栏需要处理计数为 2，只剩另一回合成果与 gamma.txt；alpha.txt、beta.txt 文件提醒均已消失")
                ])
                try session.save(frame, "\(theme)-03-turn-read.png")
                return "看完本回合全部文件后，本回合成果提醒消失"
            }
            try await session.step("键盘打开剩余成果清除这一回合的提醒") {
                let (frame, point) = try await session.visualLocate("定位左栏运行概览 / Run overview 标题，点击以聚焦概览；不要点其他控件")
                try await session.click(local: point, on: frame)
                // 两行剩余提醒：两次向上保证键盘光标到第一条成果行。
                for _ in 0..<2 { try await session.shortcut(key: 126, modifiers: []) }
                try await session.shortcut(key: 36, modifiers: [])
                let cleared = try await session.visualCheck([
                    VisualCriterion(id: "cleared", text: "左栏需要你处理 / Needs attention 计数为 0，不再显示成果或文件待查看行；右侧已经显示 gamma.txt 的冻结正文 Frozen gamma.txt；不能是加载错误或空工作台")
                ])
                try session.save(cleared, "\(theme)-04-cleared.png")
                return "回车打开成果与鼠标打开文件共享已读行为"
            }
            try await session.step("重启后已读提醒不复活，成果和源文件仍保留") {
                session.stop()
                try session.launch(language: language)
                try await session.resize(width: 1280, height: 800)
                try await enterOverview(session)
                let frame = try await session.visualCheck([
                    VisualCriterion(id: "persisted", text: "重启后左栏需要你处理 / Needs attention 计数仍为 0，没有成果或文件待查看行")
                ])
                try session.save(frame, "\(theme)-05-restarted.png")
                let expected = language == "en" ? 4 : 2
                guard try SQLRaw.scalar(database: database, "SELECT count(*) FROM mobile_task_deliveries") == expected else {
                    throw E2EError.product("已读操作不能删除冻结成果")
                }
                guard FileManager.default.fileExists(atPath: root.appendingPathComponent("alpha.txt").path),
                      FileManager.default.fileExists(atPath: root.appendingPathComponent("gamma.txt").path) else {
                    throw E2EError.product("已读操作不能删除项目文件")
                }
                return "已读持久化；冻结成果与项目文件保留"
            }
        }
    }

    private static func enterOverview(_ session: Session) async throws {
        try await session.activate()
        try await session.visualClick("点击首页中部的搜索工作空间 / Search workspaces 输入框")
        try await session.selectAll()
        try await session.type("AttentionRead")
        try await session.shortcut(key: 36, modifiers: [])
        _ = try await session.visualCheck([
            VisualCriterion(id: "workspace", text: "已进入 AttentionRead 工作空间，中央画布有 Review Project 项目；如果还在过渡则等待")
        ])
        try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
    }
}

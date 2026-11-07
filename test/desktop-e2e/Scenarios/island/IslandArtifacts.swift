import E2EKit
import Fixtures
import Foundation

/// 从另一工作空间处理冻结成果；已读只消费提醒，不删除成果或源文件。
@MainActor
public enum IslandArtifacts {
    public nonisolated static let name = "island-artifacts"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
        let project = dataRoot.appendingPathComponent("island-artifacts")
        session.stop()
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        try ContentFixture.git(project, ["init"])
        try Catalog.seedWorkspace(database: database, name: "Island artifacts")
        try Catalog.seedWorkspace(database: database, name: "Other workspace")
        try Catalog.seedArea(database: database, workspaceName: "Island artifacts", root: project, displayName: "Review Project")
        try Catalog.suppressFeatureGuide(database: database)
        try AttentionFixture.seed(database: database, root: project, cycle: "island")
        // 冻结后提交 gamma，使打开必须读取冻结正文，不能用当前 git diff 冒充成果。
        try ContentFixture.git(project, ["add", "gamma.txt"])
        try ContentFixture.git(project, ["-c", "user.name=Norma E2E", "-c", "user.email=e2e@example.invalid", "commit", "-m", "Freeze gamma"])
        try session.launch(language: "zh-Hans")
        try await session.visualClick("点击首页Other workspace卡片，进入另一工作空间")
        _ = try await session.visualCheck([
            VisualCriterion(id: "other", text: "主窗口已进入Other workspace，顶部工作空间名称为Other workspace，没有进入Review Project")
        ])
        let windows = try await session.nativeWindows()
        guard let main = windows.first(where: { $0.width > 900 }),
              let island = windows.first(where: { $0.width >= 300 && $0.width < 700 }) else {
            throw E2EError.product("缺少主窗口或独立岛")
        }
        try await session.step("另一个工作空间仍能看到全部冻结成果") {
            try await session.selectNativeWindow(island.number)
            try await expand(session)
            try await session.visualClick("点击需要你处理页签")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "unread", text: "需要你处理页签显示Island artifacts分组与Review Project的成果；有包含alpha.txt、beta.txt的成果行和标记已读、在画布查看按钮。更多条目可滚动")
            ]), "01-unread.png")
            return "非当前工作空间的成果可直接处理"
        }
        try await session.step("标记一个回合已读，不影响另一回合") {
            try await session.visualClick("点击同时列出alpha.txt和beta.txt的成果卡片内标记已读按钮；不要点单个文件条目或在画布查看")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "read", text: "出现已标记为已读反馈；alpha.txt和beta.txt及其回合成果提醒消失；gamma.txt所在另一回合成果和文件提醒仍存在")
            ]), "02-read.png")
            guard try SQLRaw.scalar(database: database, "SELECT count(*) FROM mobile_task_deliveries") == 2 else {
                throw E2EError.product("标记已读删除了冻结成果")
            }
            return "只消费目标版本的两文件提醒，保留其他回合与全部成果"
        }
        try await session.step("从岛跨空间打开冻结成果") {
            try await session.visualClick("点击gamma.txt对应的成果卡片内在画布查看按钮")
            try await session.selectNativeWindow(main.number)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "destination", text: "主窗口已切换到Island artifacts工作空间和Review Project，右侧打开gamma.txt正文Frozen gamma.txt island；不能是Other workspace、空内容或错误")
            ]), "03-opened.png")
            try await session.selectNativeWindow(island.number)
            try await expand(session)
            try await session.visualClick("点击需要你处理页签")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "empty", text: "需要你处理页签显示暂无需要你处理的事项，没有alpha、beta、gamma成果或文件提醒")
            ]), "04-cleared.png")
            return "跳回来源空间并打开真实冻结正文，成功打开后移除最后提醒"
        }
        try await session.step("重启后已读持久化，原始成果与文件保留") {
            session.stop()
            try session.launch(language: "en")
            _ = try await session.capturePixels()
            guard let island = try await session.nativeWindows().first(where: { $0.width >= 300 && $0.width < 700 }) else {
                throw E2EError.product("重启后缺少岛窗口")
            }
            try await session.selectNativeWindow(island.number)
            try await expand(session)
            try await session.visualClick("点击Needs you页签")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "persisted", text: "英文岛显示Needs you、Running、Staff三个页签；待办为空，没有alpha、beta、gamma提醒或中文残留")
            ]), "05-restarted-en.png")
            guard try SQLRaw.scalar(database: database, "SELECT count(*) FROM mobile_task_deliveries") == 2 else {
                throw E2EError.product("重启后冻结成果丢失")
            }
            for file in ["alpha.txt", "beta.txt", "gamma.txt"] {
                guard try String(contentsOf: project.appendingPathComponent(file), encoding: .utf8) == "Frozen \(file) island" else {
                    throw E2EError.product("成果操作修改或删除源文件\(file)")
                }
            }
            return "已读跨重启保留；双语界面和三个源文件均验证"
        }
    }

    private static func expand(_ session: Session) async throws {
        let frame = try await session.capturePixels()
        if frame.windowFrame.height <= 60 {
            try await session.visualClick("点击黑色胶囊中央区域，展开灵动岛")
        }
    }
}

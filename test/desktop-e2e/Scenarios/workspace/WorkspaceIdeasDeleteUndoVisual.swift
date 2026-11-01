import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 模型看图版：不调用系统 Vision OCR；定位与画面判断由当前多模态 Agent 完成，数据由程序断言。
@MainActor
enum WorkspaceIdeasDeleteUndoVisual {
    static func run(_ session: Session) async throws {
        var project: URL?
        let fakeShell = try FakeAgentShell.install()
        setenv("ZDOTDIR", fakeShell.path, 1)
        defer {
            session.stop()
            if let project { try? FileManager.default.removeItem(at: project) }
            try? FileManager.default.removeItem(at: fakeShell)
        }
        let workspace = "IdeasDelUndo"
        let title = "待删除的想法"
        try await session.step("建立含项目区与一条想法的隔离工作空间") {
            try session.launch(language: "zh-Hans")
            let db = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 20) { _ in FileManager.default.fileExists(atPath: db.path) }
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            try Catalog.seedWorkspace(database: db, name: workspace)
            try Catalog.suppressFeatureGuide(database: db)
            project = try CanvasProjectFixture.install(dataRoot: data, workspaceName: workspace)
            try Catalog.seedIdea(database: db, workspaceName: workspace, body: title, position: 1024)
            try session.launch(language: "zh-Hans")
            try await session.resize(width: 1440, height: 840)
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }
        try await session.step("进入项目并打开想法栏") {
            try await session.visualClick("定位起始页「最近访问」下的 IdeasDelUndo 工作空间卡片中心，点击进入工作空间；不要点击「新建工作空间」")
            try await session.visualClick("定位中央 E2E Project 项目卡片左上角的标题，点击进入项目；不要点击顶部导航")
            try await session.shortcut(key: 34, modifiers: [0x37, 0x38])
            let frame = try await session.visualCheck([
                VisualCriterion(id: "drawer", text: "左侧想法抽屉已打开，可见想法「\(title)」和底部「写下一个想法」输入框")
            ])
            try session.save(frame, "00-drawer.png")
            return "想法栏已打开"
        }
        try await session.step("悬停想法行：删除图标可见") {
            let (frame, point) = try await session.visualLocate("定位想法「\(title)」行的标题文字中心")
            try await session.hover(local: point, on: frame)
            let hovered = try await session.visualCheck([
                VisualCriterion(id: "actions", text: "悬停的想法行右侧出现「指派」胶囊和紧随其后的删除图标，× 形状清晰可见，不是空白灰块或缺失图标"),
                VisualCriterion(id: "row", text: "该行有悬停背景，文字不被按钮遮挡")
            ])
            try session.save(hovered, "01-row-hover.png")
            return "悬停态已看图"
        }
        try await session.step("悬停删除图标：圆形状态层") {
            let (frame, point) = try await session.visualLocate("定位想法行右侧删除图标（×）的中心，在「指派」胶囊右边")
            try await session.hover(local: point, on: frame)
            let hovered = try await session.visualCheck([
                VisualCriterion(id: "state-layer", text: "删除图标背后是浅色圆形状态层（Material 图标按钮悬停态），× 仍清晰可见；出现「删除」提示")
            ])
            try session.save(hovered, "02-delete-hover.png")
            return "删除按钮悬停态已看图"
        }
        try await session.step("点击删除：出现 Snackbar") {
            try await session.visualClick("点击想法行右侧删除图标（×）的中心")
            let snackbar = try await session.visualCheck([
                VisualCriterion(id: "snackbar", text: "抽屉底部出现反色（与抽屉明暗相反）的 Snackbar，小圆角带投影，文案「已删除「\(title)」」单行不换行，右侧是强调色「撤销」文字按钮"),
                VisualCriterion(id: "spacing", text: "Snackbar 与上方「写下一个想法」输入框有间距、不重叠，左右边距与抽屉内容对齐"),
                VisualCriterion(id: "contrast", text: "Snackbar 文字与「撤销」按钮在该底色上对比度足够、清晰可读"),
                VisualCriterion(id: "removed", text: "想法列表中不再显示「\(title)」")
            ])
            try session.save(snackbar, "03-snackbar.png")
            return "Snackbar 已看图"
        }
        try await session.step("点击撤销：想法恢复") {
            try await session.visualClick("点击 Snackbar 右侧的「撤销」按钮")
            let restored = try await session.visualCheck([
                VisualCriterion(id: "restored", text: "想法「\(title)」重新出现在列表中，Snackbar 已消失")
            ])
            try session.save(restored, "04-restored.png")
            return "已恢复"
        }
    }
}

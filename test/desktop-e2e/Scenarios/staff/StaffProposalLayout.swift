import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 长说明与待采用建议的边界；仅替代外部 ACP，任务仍由生产 CLI 创建。
@MainActor
public enum StaffProposalLayout {
    public nonisolated static let name = "staff-proposal-layout"
    public nonisolated static let englishName = "staff-proposal-layout-en"

    public static func run(_ session: Session, english: Bool = false) async throws {
        let root = try StaffFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            if let output = session.output {
                try? FileManager.default.copyItem(at: root, to: output.appendingPathComponent("staff-fixture-evidence"))
            }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            for path in [root, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", root.path, 1)
        setenv("NORMA_BRAIN_DIR", root.path, 1)
        setenv("CLAUDE_CONFIG_DIR", root.appendingPathComponent("claude-config").path, 1)
        let language = english ? "en" : "zh-Hans"
        try session.launch(language: language)
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Proposal Layout")
        try Catalog.suppressFeatureGuide(database: database)
        try VoiceConfigurationFixture.theme(database: database, id: english ? "sapphire-night" : "ink-black")
        project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Proposal Layout")
        try session.launch(language: language)
        try await session.step("长说明之后完整显示建议卡") {
            try await session.visualClick("点击 Proposal Layout 工作空间卡片")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击顶部幕僚/Staff按钮")
            try await session.visualClick("点击中央任务消息输入框")
            try await session.type("PROPOSAL-LAYOUT 请检查说明与建议卡的间距")
            try await session.shortcut(key: 36, modifiers: [])
            for _ in 0..<100 {
                let tasks = try StaffFixture.tasks(root)
                if let messages = tasks.first?["messages"] as? [[String: Any]], messages.contains(where: { $0["Norma"] != nil }) { break }
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            for width in [1280, 900, 1280] {
                try await session.resize(width: CGFloat(width), height: 840)
                try await session.scroll(xFraction: 0.50, yFraction: 0.55, deltaY: -3000)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "separation", text: "Norma说明文字的最后一段（含LAYOUT END）完整可读；其下才是Norma的建议/Norma’s suggestion卡片，正文最后一行与卡片之间有清楚的约24px空隙，没有文字穿进、压住卡片或被卡片盖住。"),
                    VisualCriterion(id: "proposal", text: "建议卡内容完整：标题归档不显示这条提示、理由仅修复布局重叠、任务指令保持卡片完整可读、建议交给/Assign to的Claude和模型选择器，以及采用/忽略/修改按钮；文字不互相重叠、不被裁切或挤没。若说明末段在视口上方，应wait后调整滚动。")
                ]), "layout-\(width)-\(UUID().uuidString.prefix(4)).png")
            }
            guard let task = try StaffFixture.tasks(root).first,
                  task["archived"] as? Bool == false,
                  (task["proposals"] as? [[String: Any]])?.count == 1,
                  try StaffFixture.records(root, "received.jsonl").isEmpty else {
                throw E2EError.product("布局验收不能归档任务、删除提议或自动派发")
            }
            return "宽窗、窄窗和恢复宽窗时正文与完整建议分开；任务未归档或派发"
        }
        try await session.step("采用后长说明与已派发单行卡分开") {
            try await session.resize(width: 1280, height: 840)
            try await session.visualClick("点击建议卡的采用并派发/Adopt and dispatch按钮")
            for _ in 0..<100 {
                if (try StaffFixture.tasks(root).first?["proposals"] as? [[String: Any]])?.first?["dispatch_id"] is String { break }
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            guard (try StaffFixture.tasks(root).first?["proposals"] as? [[String: Any]])?.first?["dispatch_id"] is String else {
                throw E2EError.product("采用后提议未记录派发")
            }
            for width in [1280, 900, 600] {
                try await session.resize(width: CGFloat(width), height: 840)
                try await session.scroll(xFraction: 0.50, yFraction: 0.55, deltaY: -3000)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "dispatched-separation", text: "Norma说明文字最后一段（含LAYOUT END）完整可读，末行与其下的已派发单行卡之间有清楚的约24px空隙；卡片没有压住或盖住任何说明文字，卡片左上角也没有露出说明里的引号或其他文字。若说明末段在视口上方，应wait后调整滚动。"),
                    VisualCriterion(id: "dispatched-row", text: "已派发卡只显示宠物头像、标题归档不显示这条提示（过长时以省略号截断）、「智能体名 · 引擎名」和绿色「✓ 已派发」/Dispatched；标题与右侧智能体名、状态之间互不遮挡；宽度不足时右侧整组换到下一行，文字不叠字、不被裁切、不超出卡片。")
                ]), "dispatched-\(width)-\(UUID().uuidString.prefix(4)).png")
            }
            return "采用后宽窗、窄窗与紧凑宽度下说明与已派发卡分开，卡内标题和状态不重叠"
        }
    }
}

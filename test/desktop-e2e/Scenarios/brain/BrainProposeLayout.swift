import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// “需要你处理”里下一步提议行的版式与交互：无多余边框、拒绝/采纳单独一行且尺寸协调、
/// 文字不截断，并覆盖悬停、按下、键盘焦点、禁用与点击行为。全程模型看图，不用 OCR。
/// 主题与语言由 `NORMA_BRAIN_PROPOSAL_THEME`（默认 ink-black）与 `NORMA_E2E_LANGUAGE`（默认 zh-Hans）选择。
@MainActor
public enum BrainProposeLayout {
    public nonisolated static let name = "brain-propose-layout"

    public static func run(_ session: Session) async throws {
        let env = ProcessInfo.processInfo.environment
        let theme = env["NORMA_BRAIN_PROPOSAL_THEME"] ?? "ink-black"
        let language = env["NORMA_E2E_LANGUAGE"] ?? "zh-Hans"
        let english = language == "en"
        let ignore = english ? "Reject" : "拒绝"
        let accept = english ? "Accept" : "采纳"
        var projectRoot: URL?
        var fakeShell: URL?
        var brainDir: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
            if let brainDir { try? FileManager.default.removeItem(at: brainDir) }
        }
        let workspace = "Proposals"

        try await session.step("播种工作空间、项目与主题 \(theme)，开启只提议的脚本化大脑") {
            fakeShell = try BrainFixture.installFakeAgents()
            brainDir = try BrainFixture.installScriptedBrainDirectory(mode: .propose)
            setenv("ZDOTDIR", fakeShell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brainDir!.path, 1)
            try session.launch(language: language)
            let db = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 20) { _ in FileManager.default.fileExists(atPath: db.path) }
            session.stop()
            try Catalog.seedWorkspace(database: db, name: workspace)
            try Catalog.suppressFeatureGuide(database: db)
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try Catalog.seedSetting(database: db, key: "appearance.theme.v1", value: theme)
            try session.launch(language: language)
            try await session.resize(width: 1440, height: 840)
            try await session.activate()
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入项目并创建两个智能体，等脚本化大脑写出两条提议") {
            try await session.visualClick("点击首页中部的搜索工作空间 / Search workspaces 输入框")
            try await session.selectAll()
            try await session.type(workspace)
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await session.visualCheck([
                VisualCriterion(id: "workspace", text: "已进入 Proposals 工作空间，画布上有 E2E Project 项目；若仍在过渡则 wait")
            ])
            try await session.visualClick("点击画布上名为 E2E Project 的项目")
            _ = try await session.visualCheck([
                VisualCriterion(id: "project", text: "已进入 E2E Project 项目画布，底部有指令输入栏；若有功能引导遮挡或仍在加载则 wait")
            ])
            // 模型定位一次、之后复用坐标：两个智能体必须在 Alan 完成前都存在，脚本化大脑才会提两条。
            let (dockFrame, dockPoint) = try await session.visualLocate("定位画布底部居中指令输入栏的文字输入区中心")
            for _ in 0..<2 {
                try await session.click(local: dockPoint, on: dockFrame)
                try await session.selectAll()
                try await session.type("/new claude")
                try await session.shortcut(key: 36, modifiers: [])
                try await Task.sleep(nanoseconds: 2_500_000_000)
            }
            let deadline = Date().addingTimeInterval(120)
            while Date() < deadline {
                if (try? await BrainPropose.savedProposals(brainDir, expect: 2)) == 2 { break }
                try await Task.sleep(nanoseconds: 2_000_000_000)
            }
            guard try await BrainPropose.savedProposals(brainDir, expect: 2) == 2 else {
                throw E2EError.product("提议文件里应有 2 条")
            }
            try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
            let frame = try await session.visualCheck([
                VisualCriterion(id: "rows", text: "左栏需要你处理 / Needs attention 里有两条下一步提议，标题带 ✦；若还没出现则 wait"),
                VisualCriterion(id: "borders", text: "提议行之间和行内没有多余的描边框或分隔线；只有无边框的行背景"),
                VisualCriterion(id: "buttons", text: "每条提议的\(ignore)与\(accept)按钮在标题下方单独一行并靠右，高度明显小于标题行，和周围文字协调，不挤占标题宽度"),
                VisualCriterion(id: "text", text: "提议标题与副标题完整显示，没有被按钮挤成三行或截断，按钮文字完整")
            ])
            try session.save(frame, "01-layout-\(theme)-\(language).png")
            return "两条提议版式通过"
        }

        try await session.step("悬停与键盘焦点状态") {
            let (frame, point) = try await session.visualLocate("定位第一条提议下方的“\(ignore)”按钮中心")
            try await session.hover(local: point, on: frame)
            let hovered = try await session.visualCheck([
                VisualCriterion(id: "hover", text: "鼠标悬停的\(ignore)按钮底色比未悬停的按钮明显更亮或更深，与其他按钮有可见区别")
            ])
            try session.save(hovered, "02-hover-button-\(theme)-\(language).png")
            let (rowFrame, rowPoint) = try await session.visualLocate("定位第一条提议的标题文字中心")
            try await session.hover(local: rowPoint, on: rowFrame)
            let row = try await session.visualCheck([
                VisualCriterion(id: "row-hover", text: "悬停的提议标题行出现可见的淡色底色，没有描边框，其他行不受影响")
            ])
            try session.save(row, "03-hover-row-\(theme)-\(language).png")
            // 概览内依次为图钉、三个统计卡、第一条提议标题；必须验证第五个 Tab stop。
            let (titleFrame, titlePoint) = try await session.visualLocate("定位左栏顶部的“运行概览 / Run Overview”标题文字中心")
            try await session.click(local: titlePoint, on: titleFrame)
            try await session.hover(local: titlePoint, on: titleFrame)
            for _ in 0..<5 { try await session.shortcut(key: 48, modifiers: []) }
            let focused = try await session.visualCheck([
                VisualCriterion(id: "focus", text: "鼠标停在概览标题处，第一条提议标题行有区别于第二条标题行的底色高亮，且没有描边；若仅图钉、统计卡或按钮有焦点，或标题无底色高亮、出现描边，则 fail")
            ])
            try session.save(focused, "04-keyboard-focus-\(theme)-\(language).png")
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await session.visualCheck([
                VisualCriterion(id: "keyboard-expand", text: "按 Enter 后第一条提议展开并显示原因正文，第二条仍收起；否则 fail")
            ])
            try await session.shortcut(key: 49, modifiers: [])
            _ = try await session.visualCheck([
                VisualCriterion(id: "keyboard-collapse", text: "按 Space 后第一条提议收起，原因正文不再可见，两条提议及按钮均保留；否则 fail")
            ])
            return "提议标题焦点底色可见，Enter/Space 展开收起目标正确"
        }

        try await session.step("展开第一条提议，按钮仍在标题下方且内容不截断") {
            let (frame, point) = try await session.visualLocate("定位第一条提议的标题文字中心")
            try await session.click(local: point, on: frame)
            let expanded = try await session.visualCheck([
                VisualCriterion(id: "expanded", text: "第一条提议展开，显示行动、原因、交付等完整内容；英文 Reason / Output 标签各自单行横排，不能逐字竖排；\(ignore)/\(accept)按钮仍在标题下方一行，没有叠到正文上，文字不截断")
            ])
            try session.save(expanded, "05-expanded-\(theme)-\(language).png")
            try await session.click(local: point, on: expanded)
            return "展开布局正常"
        }

        try await session.step("按下态并点击“\(ignore)”：第一条消失，另一条保留") {
            let (frame, point) = try await session.visualLocate("定位第一条提议下方的“\(ignore)”按钮中心")
            try await session.press(local: point, on: frame) {
                let pressed = try await session.visualCheck([
                    VisualCriterion(id: "pressed", text: "鼠标按住的\(ignore)按钮底色比悬停时更深或更亮，按下反馈可见")
                ])
                try session.save(pressed, "06-pressed-\(theme)-\(language).png")
            }
            let after = try await session.visualCheck([
                VisualCriterion(id: "ignored", text: "只剩一条提议（补登录页测试），第一条已消失；待处理计数为 1")
            ])
            try session.save(after, "07-ignored-\(theme)-\(language).png")
            guard try await BrainPropose.savedProposals(brainDir, expect: 1) == 1 else {
                throw E2EError.product("忽略后提议文件应剩 1 条")
            }
            return "拒绝生效"
        }

        try await session.step("目标等待确认时采纳禁用并给出原因，解除后可点击采纳") {
            guard let brainDir else { throw E2EError.env("大脑夹具目录丢失") }
            let marker = brainDir.appendingPathComponent("waiting-2")
            try Data().write(to: marker)
            try await Task.sleep(nanoseconds: 3_000_000_000)
            let blocked = try await session.visualCheck([
                VisualCriterion(id: "disabled", text: "\(accept)按钮呈灰色禁用态，文字淡；提议下方有说明需要先处理卡片问题或审批的提示，完整不截断；\(ignore)仍可用")
            ])
            try session.save(blocked, "08-disabled-\(theme)-\(language).png")
            try await session.visualClick("点击剩余提议的禁用按钮，文字为等待确认 / Waiting，不要点击拒绝")
            try await Task.sleep(nanoseconds: 1_000_000_000)
            guard try BrainPropose.received(brainDir).isEmpty,
                  try await BrainPropose.savedProposals(brainDir, expect: 1) == 1 else {
                throw E2EError.product("点击禁用按钮后不得派发或消费提议")
            }
            try FileManager.default.removeItem(at: marker)
            try await Task.sleep(nanoseconds: 3_000_000_000)
            try await session.visualClick("点击剩余那条提议的“\(accept)”按钮")
            let deadline = Date().addingTimeInterval(10)
            var messages = try BrainPropose.received(brainDir)
            while Date() < deadline && messages.isEmpty {
                try await Task.sleep(nanoseconds: 200_000_000)
                messages = try BrainPropose.received(brainDir)
            }
            let done = try await session.visualCheck([
                VisualCriterion(id: "dispatched", text: "需要你处理里不再有下一步提议行")
            ])
            try session.save(done, "09-dispatched-\(theme)-\(language).png")
            messages = try BrainPropose.received(brainDir)
            guard messages.count == 1, messages[0]["node"] as? Int == 2,
                  messages[0]["task"] as? String == "补上登录页的测试，cargo test 通过即完成",
                  try await BrainPropose.savedProposals(brainDir, expect: 0) == 0 else {
                throw E2EError.product("目标没有且仅收到一次完整原文：\(messages)")
            }
            guard let output = session.output else { throw E2EError.env("验收证据目录丢失") }
            try Data(contentsOf: brainDir.appendingPathComponent("received.jsonl"))
                .write(to: output.appendingPathComponent("received.jsonl"))
            return "Bella（node 2）收到一次完整原文，提议已清空"
        }
    }
}

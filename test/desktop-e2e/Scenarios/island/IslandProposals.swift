import E2EKit
import Fixtures
import Foundation

/// 幕僚提议出现在岛的「需要你处理」：计数、阻断原因、派发、忽略和中英两种工作空间主题。
@MainActor
public enum IslandProposals {
    public nonisolated static let name = "island-proposals"

    public static func run(_ session: Session) async throws {
        let brain = try IslandStaffFixture.install(real: false)
        let shell = try IslandProposalsFixture.installShell()
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        var project: URL?
        defer {
            session.stop()
            for key in keys {
                if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) }
            }
            for path in [brain, shell, project].compactMap({ $0 }) {
                try? FileManager.default.removeItem(at: path)
            }
        }
        setenv("ZDOTDIR", shell.path, 1)
        setenv("NORMA_BRAIN_DIR", brain.path, 1)

        let agent = try await prepareAgent(session, project: &project)
        let proposalFile = try seedProposals(session, brain: brain, agent: agent, marker: "ISLAND-PROPOSAL-OK")
        try Catalog.seedSetting(database: try session.databaseURL(), key: "appearance.theme.v1", value: "sapphire-night")

        try await session.step("深色工作空间里，两条提议出现在需要你处理，计数为 2") {
            try session.launch(language: "zh-Hans")
            try await openProject(session)
            try await session.selectNativeWindow(nil)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "dark-workspace", text: "工作空间画布是深色。若仍在进入项目，判 wait。")
            ]), "01-dark-workspace.png")
            try await focusIsland(session)
            try await session.visualClick("点击顶部黑色收起胶囊的正中，把岛展开。不要点主窗口，不要点菜单栏")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "pending-zh", text: "岛仍是黑色表面。「需要你处理」页签选中，旁边有橙色数字 2。两张提议卡：Alan 的标题是整理日志，徽章写提议，浅色实心「派发」可点，深色「忽略」可点。Nobody 的标题是未知目标，派发发灰禁用，橙色原因是「未找到 Agent：Nobody」，忽略仍可点。没有审批弹层。若提议还没出现，判 wait。")
            ]), "02-pending-zh.png")
            return "深色工作空间下两条提议和禁用原因可见"
        }

        try await session.step("悬停可点的派发，按下 Nobody 的忽略") {
            let dispatch = try await session.visualLocate("定位 Alan「整理日志」这张卡上浅色「派发」按钮的中心。不要选 Nobody，不要选忽略")
            try await session.hover(local: dispatch.point, on: dispatch.frame, dwell: 400_000_000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "hover-zh", text: "Alan 的派发按钮比旁边未悬停的按钮更亮或更白，仍是浅色实心。Nobody 的派发仍然发灰。两张卡都还在。")
            ]), "03-dispatch-hover.png")
            let ignore = try await session.visualLocate("定位 Nobody「未知目标」这张卡上的「忽略」按钮中心。不要选派发，不要选 Alan")
            try await session.press(local: ignore.point, on: ignore.frame) {
                try session.save(try await session.visualSnapshotCheck([
                    VisualCriterion(id: "pressed-zh", text: "Nobody 的忽略按钮正处于按下，颜色比普通深色按钮更深。Alan 的卡还在。")
                ]), "04-ignore-pressed.png")
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "dismissed-zh", text: "Nobody 的未知目标卡已经消失。Alan 的整理日志还在。能看到「已忽略这条提议」。需要你处理旁边的数字变成 1。没有审批弹层。若忽略尚未生效，判 wait。")
            ]), "05-dismissed-zh.png")
            return "忽略按钮有按下反馈，Nobody 被取走"
        }

        try await session.step("派发把任务交给 Alan，提议卡消失") {
            try await session.visualClick("点击 Alan「整理日志」卡上浅色可点的「派发」。不要点忽略")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "dispatched-zh", text: "整理日志和未知目标两张提议卡都没有了。岛上能看到「提议已派发」。需要你处理没有橙色数字 2。若派发还在进行，判 wait。")
            ]), "06-dispatched-zh.png")
            try await waitTitles(proposalFile, expected: [])
            try await waitDispatch(session, marker: "ISLAND-PROPOSAL-OK")
            let tasks = try session.dispatchTasks()
            guard !tasks.contains(where: { $0.taskText.contains("ISLAND-PROPOSAL-BLOCKED") }) else {
                throw E2EError.product("被忽略的 Nobody 不应留下派发：\(tasks)")
            }
            return "Alan 收到 ISLAND-PROPOSAL-OK，文件里的两条提议都已取走"
        }

        try await session.step("重启后已忽略和已派发的提议不再出现") {
            session.stop()
            try session.launch(language: "zh-Hans")
            try await openProject(session)
            try await focusIsland(session)
            try await session.visualClick("点击顶部黑色收起胶囊的正中，把岛展开")
            try await session.visualClick("点击「需要你处理」页签")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "gone-zh", text: "需要你处理页里没有整理日志，没有未知目标，没有派发或忽略按钮。能看到暂无需要你处理，或审批、提问、提议和成果的空态提示。")
            ]), "07-gone-zh.png")
            let titles = try IslandProposalsFixture.titles(proposalFile)
            guard titles.isEmpty else { throw E2EError.product("重启后提议文件仍有 \(titles)") }
            return "重启后提议不再出现"
        }

        try await session.step("浅色工作空间、英文界面仍能看出禁用派发并完成派发") {
            session.stop()
            _ = try IslandProposalsFixture.writeProposals(
                brain: brain, scope: try proposalScope(session), agent: agent, taskMarker: "ISLAND-PROPOSAL-OK-EN"
            )
            try Catalog.seedSetting(database: try session.databaseURL(), key: "appearance.theme.v1", value: "paper-blue")
            try session.launch(language: "en")
            try await openProject(session)
            try await session.selectNativeWindow(nil)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "light-workspace", text: "工作空间画布是浅色纸感背景。若仍在进入项目，判 wait。")
            ]), "08-light-workspace.png")
            try await focusIsland(session)
            try await session.visualClick("点击顶部黑色收起胶囊的正中，把岛展开。不要点主窗口")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "pending-en", text: "岛本身仍是黑色，不跟浅色工作空间变白。Needs you 页签选中，旁边橙色数字 2。Alan 卡标题整理日志，徽章 Proposal，Dispatch 是浅色实心可点，Dismiss 可点。Nobody 卡标题未知目标，Dispatch 发灰禁用，橙色原因是 Agent Nobody was not found，Dismiss 仍可点。没有审批弹层。若英文提议还没出现，判 wait。")
            ]), "09-pending-en.png")
            try await session.visualClick("点击 Nobody 那张卡上的 Dismiss。不要点 Dispatch，不要点 Alan")
            try await session.visualClick("点击 Alan 那张卡上浅色可点的 Dispatch。不要点 Dismiss")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "done-en", text: "两张提议卡都消失了。能看到 Proposal dispatched 或 Proposal dismissed。没有中文派发按钮。若还在处理，判 wait。")
            ]), "10-done-en.png")
            try await waitTitles(proposalFile, expected: [])
            try await waitDispatch(session, marker: "ISLAND-PROPOSAL-OK-EN")
            return "浅色英文下阻断原因可读，派发和忽略都完成"
        }
    }

    private static func prepareAgent(_ session: Session, project: inout URL?) async throws -> String {
        try await session.step("准备空闲的 Alan 卡片") {
            try session.launch(language: "zh-Hans")
            _ = try await session.capturePixels()
            session.stop()
            guard let root = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
            let db = try session.databaseURL()
            try Catalog.seedWorkspace(database: db, name: "Island proposals")
            try Catalog.suppressFeatureGuide(database: db)
            project = try CanvasProjectFixture.install(dataRoot: root, workspaceName: "Island proposals")
            try session.launch(language: "zh-Hans")
            try await openProject(session)
            try await session.visualClick("点击画布底部命令输入框")
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await session.visualCheck([
                VisualCriterion(id: "agent-ready", text: "Claude 卡片终端出现 PROPOSAL AGENT READY。没有权限确认框，没有启动错误。若卡片还在启动，判 wait。")
            ])
            return "脚本 Agent 已结束首轮，可以接收派发"
        }
        session.stop()
        let definitions = try session.agentDefinitions()
        guard definitions.count == 1, let name = definitions.first?.name, name == "Alan" else {
            throw E2EError.product("空名册第一张卡应为 Alan，实际 \(definitions)")
        }
        return name
    }

    private static func seedProposals(_ session: Session, brain: URL, agent: String, marker: String) throws -> URL {
        try IslandProposalsFixture.writeProposals(
            brain: brain, scope: try proposalScope(session), agent: agent, taskMarker: marker
        )
    }

    private static func proposalScope(_ session: Session) throws -> String {
        let catalog = try session.catalog()
        guard let area = catalog.areas.first(where: { $0.displayName == "E2E Project" }) else {
            throw E2EError.product("找不到 E2E Project。\(catalog.summary)")
        }
        return "desktop:\(area.workspaceId):\(area.id)"
    }

    private static func openProject(_ session: Session) async throws {
        try await session.visualClick("点击标题为 Island proposals 的工作空间卡片。点在这几个字上，不要点新建")
        try await session.visualClick("点击 E2E Project 项目卡片，进入画布")
    }

    private static func focusIsland(_ session: Session) async throws {
        let deadline = Date().addingTimeInterval(25)
        while Date() < deadline {
            if let island = try await session.nativeWindows().first(where: {
                $0.width >= 280 && $0.width < 760 && $0.height <= 80
            }) {
                try await session.selectNativeWindow(island.number)
                return
            }
            try await Task.sleep(nanoseconds: 250_000_000)
        }
        let listed = (try? await session.nativeWindows())?.map { "\($0.width)x\($0.height)" } ?? []
        throw E2EError.product("缺少收起的岛窗口。当前 \(listed)")
    }

    private static func waitTitles(_ file: URL, expected: [String]) async throws {
        let deadline = Date().addingTimeInterval(8)
        var last: [String] = []
        while Date() < deadline {
            last = (try? IslandProposalsFixture.titles(file)) ?? []
            if last.sorted() == expected.sorted() { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("提议文件标题应为 \(expected)，实际 \(last)，文件 \(file.path)")
    }

    private static func waitDispatch(_ session: Session, marker: String) async throws {
        let deadline = Date().addingTimeInterval(8)
        var last: [DispatchTaskRow] = []
        while Date() < deadline {
            last = (try? session.dispatchTasks()) ?? []
            if last.contains(where: { $0.taskText.contains(marker) }) { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("派发记录里没有 \(marker)。实际 \(last)")
    }
}

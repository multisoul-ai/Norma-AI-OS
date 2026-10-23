import E2EKit
import Fixtures
import Foundation

/// 想法排序与快捷派发：列表内拖动改序，`⌃1` 派给第 1 张卡片，`⌃⇧3` / `⌃0` 新建智能体并派发。
/// 快捷键经控制通道以 GPUI 按键描述送入，验证的是注册、分发与派发链路，不等于物理键盘上的修饰键组合。
@MainActor
public enum WorkspaceIdeasShortcuts {
    public nonisolated static let name = "workspace-ideas-shortcuts"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }
        let workspace = "IdeasKeys"
        let bodies = ["一号想法", "二号想法", "三号想法", "四号想法"]
        // 先在没展开项目时把四号拖到最前（四 一 二 三），进入项目后再把三号拖到最前：派发队列就是 三 四 一 二。（用数字而不是甲乙丙丁：OCR 会把“丁方”读成“丁万”。）
        let queue = [bodies[2], bodies[3], bodies[0], bodies[1]]

        try await session.step("建立含项目区与四条想法的隔离工作空间") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace", "Create your first"], timeout: 25))
            try await session.act("打开创建对话框", attempts: 4, expect: Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 3.5)) {
                let launch = try await session.capture()
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) / 2, y: 515), on: launch)
            }
            let dialog = try await session.capture()
            guard let input = dialog.pick(["网站改版", "Website redesign"], .bottom) else {
                throw E2EError.product("找不到工作空间名称输入框：\(dialog.summary)")
            }
            try await session.click(local: input.center, on: dialog)
            try await session.type(workspace)
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 12))
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            let database = dataRoot.appendingPathComponent("app.sqlite")
            // 预置默认新建类型：启动后 ⌃0 应新建 ZCode 卡片，证明设置被读回。
            try Catalog.seedSetting(database: database, key: "ideas.new_agent_profile.v1", value: "zcode")
            for (offset, body) in bodies.enumerated() {
                try Catalog.seedIdea(database: database, workspaceName: workspace, body: body, position: 1024 * (offset + 1))
            }
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return "已写入 4 条想法"
        }

        func foldOtherGroups() async throws {
            // 折叠其余三栏（需要你处理 / 运行中 / 待开始），想法栏拿到全部剩余高度，四条想法都可见。
            for title in ["要你处理", "运行中", "待开始"] {
                let frame = try await session.capture()
                guard let heading = frame.texts.filter({ $0.value.contains(title) && $0.center.x < 100 }).min(by: { $0.center.y < $1.center.y }) else {
                    throw E2EError.product("找不到栏标题「\(title)」：\(frame.summary)")
                }
                try await session.click(local: heading.center, on: frame)
                try await Task.sleep(nanoseconds: 400_000_000)
            }
        }
        func ideaRows() throws -> [(body: String, state: String, taskRef: String)] {
            try Catalog.ideas(database: try session.databaseURL()).filter { bodies.contains($0.body) }
        }
        func row(_ frame: Frame, _ body: String) throws -> VisibleText {
            // OCR 会把「丙」读成「内」，两种读法都算同一行。
            let keys = [String(body.prefix(2))] + (body.hasPrefix("丙") ? ["内方"] : [])
            guard let text = frame.texts.filter({ text in keys.contains { text.value.contains($0) } && text.center.x < 440 }).first else {
                throw E2EError.product("找不到想法行「\(body)」：\(frame.summary)")
            }
            return text
        }

        try await session.step("没有展开项目：想法栏里仍可拖动排序，库里顺序立即改变") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540), on: launch)
                _ = try await session.wait(Expectation(absent: ["搜索工作空间", "Search workspaces"], timeout: 12))
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await Task.sleep(nanoseconds: 900_000_000)
            let entered = try await session.capture()
            if entered.contains("Feature Guide") || entered.contains("Skip Guide") {
                try await session.shortcut(key: 53, modifiers: [])
                _ = try await session.wait(Expectation(absent: ["Feature Guide", "Skip Guide"], timeout: 6))
            }
            try await session.shortcut(key: 34, modifiers: [0x37, 0x38])
            let opened = try await session.wait(Expectation(allOf: [String(bodies[0].prefix(2))], timeout: 8))
            try session.save(opened, "00-no-project.png")
            try await foldOtherGroups()
            let frame = try await session.wait(Expectation(allOf: [String(bodies[0].prefix(2)), String(bodies[3].prefix(2))], timeout: 6))
            let first = try row(frame, bodies[0])
            let last = try row(frame, bodies[3])
            try await session.drag(local: last.center, to: CGPoint(x: first.center.x, y: first.center.y - 14), on: frame)
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let order = try ideaRows().map(\.body)
            try session.save(try await session.capture(), "00b-no-project-reordered.png")
            guard order == [bodies[3], bodies[0], bodies[1], bodies[2]] else {
                throw E2EError.product("没展开项目时拖动后顺序应为 四 一 二 三，实际 \(order)")
            }
            guard try ideaRows().allSatisfy({ $0.state == "todo" }) else {
                throw E2EError.product("排序不应派发任何想法：\(try ideaRows())")
            }
            return "\(order)"
        }

        try await session.step("进入项目并创建一个智能体，打开想法栏") {
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            let entered = try await session.capture()
            if entered.contains("Feature Guide") || entered.contains("Skip Guide") {
                try await session.shortcut(key: 53, modifiers: [])
                _ = try await session.wait(Expectation(absent: ["Feature Guide", "Skip Guide"], timeout: 6))
            }
            // ⌘2 新建 Claude 卡片。不经底部指令栏输入回车：默认的 GPUI 注入路径会给按键补一个模拟 IME 的 "\n"，
            // 而单行输入框的 Enter 动作会继续传播，Run Overview 打开时换行被插进指令栏而崩溃（只出现在注入路径；NORMA_E2E_INPUT=ns 的真实 AppKit 路径正常）。
            try await session.shortcut(key: 19, modifiers: [0x37])
            try await Task.sleep(nanoseconds: 2_500_000_000)
            // 想法栏已在上一步打开并折叠过其余三栏：此时应仍是展开的同一布局。
            let ideas = try await session.wait(Expectation(allOf: [String(bodies[0].prefix(2)), String(bodies[3].prefix(2))], timeout: 8))
            try session.save(ideas, "01-ideas.png")
            return ideas.summary
        }

        try await session.step("展开项目后列表内拖动：把最后一条（丙）拖到第一条之前，库里顺序立即改变") {
            let frame = try await session.capture()
            let first = try row(frame, bodies[3])
            let last = try row(frame, bodies[2])
            try await session.drag(local: last.center, to: CGPoint(x: first.center.x, y: first.center.y - 14), on: frame)
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let order = try ideaRows().map(\.body)
            try session.save(try await session.capture(), "02-reordered.png")
            guard order == queue else {
                throw E2EError.product("拖动后顺序应为 三 四 一 二，实际 \(order)")
            }
            guard try ideaRows().allSatisfy({ $0.state == "todo" }) else {
                throw E2EError.product("列表内排序不应派发任何想法：\(try ideaRows())")
            }
            return "\(order)"
        }

        try await session.step("按住 ⌃：卡片左上角浮出序号；按住 ⌃⇧：出现新建类型提示；松开后都消失") {
            let frame = try await session.capture()
            let width = Double(frame.windowFrame.width)
            let drawer = min(438, max(366, width * 0.31))
            let size = frame.windowFrame.size
            let corner = CGRect(x: drawer + 22 - 12, y: 72 - 17, width: 36, height: 36)
            let baseline = try await session.snapshot()
            try await session.holdModifiers([0x3B])
            let held = try await session.snapshot()
            try session.saveSnapshot(held, "02b-ctrl-held.png")
            let shown = changedPixels(baseline, held, in: corner, windowSize: size)
            try await session.holdModifiers([])
            let released = try await session.snapshot()
            // 运行中的卡片顶边有动画，不与松手前的基线比；改看“按住与松开之间”的差异是否就是序号本身。
            let gone = changedPixels(held, released, in: corner, windowSize: size)
            guard shown >= 150 else { throw E2EError.product("按住 ⌃ 后第 1 张卡片左上角没有出现序号（变化像素 \(shown)）") }
            guard gone >= 150 else { throw E2EError.product("松开 ⌃ 后序号没有消失（按住与松开的差异仅 \(gone) 像素）") }
            try await session.holdModifiers([0x3B, 0x38])
            let hint = try await session.wait(Expectation(allOf: ["Hermes"], timeout: 4))
            try session.save(hint, "02c-ctrl-shift-held.png")
            try await session.holdModifiers([])
            _ = try await session.wait(Expectation(absent: ["Hermes"], timeout: 4))
            return "序号像素 \(shown)，松开后差异 \(gone)"
        }

        func assertAssigned(_ body: String, _ expectedTodo: [String]) async throws -> String {
            try await Task.sleep(nanoseconds: 2_500_000_000)
            let rows = try ideaRows()
            guard let found = rows.first(where: { $0.body == body }), found.state == "assigned", !found.taskRef.isEmpty, !found.taskRef.hasPrefix("idea:") else {
                throw E2EError.product("「\(body)」应已指派并带真实任务引用：\(rows)")
            }
            let todo = rows.filter { $0.state == "todo" }.map(\.body)
            guard todo == expectedTodo else { throw E2EError.product("剩余待办应为 \(expectedTodo)，实际 \(todo)") }
            return found.taskRef
        }

        try await session.step("⌃1：队首想法（丙）派给第 1 张卡片") {
            try await session.shortcut(key: 18, modifiers: [0x3B])
            let ref = try await assertAssigned(queue[0], Array(queue[1...]))
            try session.save(try await session.capture(), "03-ctrl1.png")
            return ref
        }

        try await session.step("⌃⇧3：新建 Codex 智能体并派发队首想法（丁）") {
            try await session.shortcut(key: 20, modifiers: [0x3B, 0x38])
            let ref = try await assertAssigned(queue[1], Array(queue[2...]))
            try session.save(try await session.capture(), "04-ctrl-shift3.png")
            return ref
        }

        try await session.step("⌃0：新建已保存的默认类型（ZCode）智能体并派发队首想法（甲）") {
            try await session.shortcut(key: 29, modifiers: [0x3B])
            let ref = try await assertAssigned(queue[2], [queue[3]])
            // 测试机没有 zcode 命令行：新卡片里会打印“ZCode CLI … was not found”，借此确认类型来自已保存的设置而不是 Codex。
            _ = try await session.wait(Expectation(anyOf: ["ZCode CLI", "zcode"], timeout: 6))
            try session.save(try await session.capture(), "05-ctrl0.png")
            return ref
        }

        try await session.step("⌃⇧0：弹出类型选择器（高亮在已保存的 ZCode），↓ 选到 InfCode 回车，新建 InfCode 智能体并派发队首想法（乙）") {
            try await session.shortcut(key: 29, modifiers: [0x3B, 0x38])
            let picker = try await session.wait(Expectation(allOf: ["Hermes", "ZCode", "OpenCode"], timeout: 5))
            try session.save(picker, "06-picker.png")
            try await session.shortcut(key: 125, modifiers: [])
            try await session.shortcut(key: 36, modifiers: [])
            let ref = try await assertAssigned(queue[3], [])
            _ = try await session.wait(Expectation(absent: ["OpenCode"], timeout: 4))
            try session.save(try await session.capture(), "07-zcode.png")
            return ref
        }

        try await session.step("收尾：四次派发的任务引用互不相同") {
            let rows = try ideaRows()
            let refs = rows.filter { !$0.taskRef.isEmpty }.map(\.taskRef)
            guard refs.count == 4, Set(refs).count == 4 else { throw E2EError.product("四次派发应各有独立任务引用：\(refs)") }
            return "\(rows)"
        }
    }
}

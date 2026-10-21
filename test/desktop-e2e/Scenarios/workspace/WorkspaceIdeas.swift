import E2EKit
import Fixtures
import Foundation

/// 想法清单：⌘⇧I 打开、把想法拖到智能体卡片派发，核对数据与画面。
@MainActor
public enum WorkspaceIdeas {
    public nonisolated static let name = "workspace-ideas"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }
        let workspace = "IdeasCheck"
        let first = "检查登录方案"
        let second = "整理注册流程"
        let blank = "空白落点想法"
        let cancelled = "取消拖拽的想法"
        let fresh = "交给新智能体的想法"
        let viaMenu = "走菜单指派的想法"
        let typed = "typed by hand"

        try await session.step("建立含项目区与两条想法的隔离工作空间") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace", "Create your first"], timeout: 25))
            // 启动页的「创建工作空间」按钮位于窗口水平居中、约 515px 处；OCR 偶尔识别不出，这里按位置点击。
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
            try Catalog.seedIdea(database: database, workspaceName: workspace, body: first, position: 1024)
            for (offset, body) in [second, blank, cancelled, fresh, viaMenu].enumerated() {
                try Catalog.seedIdea(database: database, workspaceName: workspace, body: body, position: 2048 + offset * 1024)
            }
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return "已写入 6 条想法"
        }

        try await session.step("进入项目并创建一个智能体") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(
                    local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540),
                    on: launch
                )
                _ = try await session.wait(Expectation(absent: ["搜索工作空间", "Search workspaces"], timeout: 12))
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                on: frame
            )
            try await session.selectAll()
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            try await Task.sleep(nanoseconds: 2_500_000_000)
            return try await session.capture().summary
        }

        try await session.step("⌘⇧I 打开运行概览的想法栏，四栏顺序固定，看到待办") {
            try await session.shortcut(key: 34, modifiers: [0x37, 0x38])
            let ideas = try await session.wait(
                Expectation(allOf: ["当前项目", "其他项目", first, second], timeout: 8)
            )
            try session.save(ideas, "01-ideas.png")
            func top(_ text: String) throws -> Double {
                guard let found = ideas.texts.filter({ $0.value.contains(text) && $0.center.x < 440 }).first else {
                    throw E2EError.product("概览里找不到栏目「\(text)」：\(ideas.summary)")
                }
                return Double(found.center.y)
            }
            let order = [try top("处理"), try top("当前项目"), try top(String(first.prefix(3))), try top("其他项目")]
            guard order == order.sorted() else {
                throw E2EError.product("栏目顺序应为 需要你处理 → 当前项目 → 想法 → 其他项目：\(order)")
            }
            return ideas.summary
        }

        try await session.step("折叠其余三栏：想法栏分到全部剩余高度，六条想法都可见") {
            for title in ["处理", "当前项目", "其他项目"] {
                let frame = try await session.capture()
                guard let heading = frame.texts.filter({ $0.value.contains(title) && $0.center.x < 440 }).first else {
                    throw E2EError.product("找不到栏标题「\(title)」：\(frame.summary)")
                }
                try await session.click(local: heading.center, on: frame)
                try await Task.sleep(nanoseconds: 400_000_000)
            }
            let folded = try await session.wait(
                Expectation(allOf: [String(first.prefix(3)), String(second.prefix(3))], timeout: 6)
            )
            try session.save(folded, "01b-folded.png")
            return folded.summary
        }

        func idea(_ body: String) throws -> (body: String, state: String, taskRef: String) {
            let all = try Catalog.ideas(database: try session.databaseURL())
            guard let found = all.first(where: { $0.body == body }) else {
                throw E2EError.product("库里找不到想法「\(body)」：\(all)")
            }
            return found
        }
        func geometry(_ frame: Frame) -> (width: Double, height: Double, drawer: Double) {
            let width = Double(frame.windowFrame.width)
            return (width, Double(frame.windowFrame.height), min(438, max(366, width * 0.31)))
        }
        func cardTarget(_ frame: Frame) throws -> CGPoint {
            // 假智能体卡片标题行含临时项目目录名；落点取标题行正下方的卡片正文区。
            guard let title = frame.pick(["norma-canvas-e2e"], .bottom) else {
                throw E2EError.product("找不到智能体卡片标题：\(frame.summary)")
            }
            return CGPoint(x: title.center.x, y: title.center.y + 150)
        }
        func row(_ frame: Frame, _ body: String) throws -> VisibleText {
            let key = String(body.prefix(3))
            guard let text = frame.texts.filter({ $0.value.contains(key) && $0.center.x < geometry(frame).drawer }).first else {
                throw E2EError.product("找不到想法行「\(body)」：\(frame.summary)")
            }
            return text
        }

        /// 想法栏固定高度、栏内滚动：目标行不在可见范围时先在栏内来回滚动再截图。
        func capture(showing body: String) async throws -> Frame {
            var frame = try await session.capture()
            for attempt in 0..<6 {
                if (try? row(frame, body)) != nil { return frame }
                let geo = geometry(frame)
                try await session.scroll(local: CGPoint(x: geo.drawer / 2, y: geo.height * 0.5), pixels: attempt % 2 == 0 ? 400 : -800, on: frame)
                try await Task.sleep(nanoseconds: 400_000_000)
                frame = try await session.capture()
            }
            return frame
        }

        try await session.step("手动添加：输入框里写一条并回车，库里是手动来源的待办") {
            let frame = try await session.capture()
            guard let input = frame.pick(["写下一个想法"], .top) else {
                throw E2EError.product("找不到想法输入框：\(frame.summary)")
            }
            try await session.click(local: input.center, on: frame)
            try await session.type(typed)
            try await session.shortcut(key: 36, modifiers: [])
            let after = try await session.wait(Expectation(allOf: ["by hand"], timeout: 8))
            try session.save(after, "02-typed.png")
            let stored = try idea(typed)
            guard stored.state == "todo" else { throw E2EError.product("手动添加的想法应是待办：\(stored)") }
            let manual = try Catalog.ideaSources(database: try session.databaseURL())[typed]
            guard manual == "manual" else { throw E2EError.product("来源应为 manual，实际 \(manual ?? "nil")") }
            return "已记下 \(typed)"
        }

        try await session.step("拖到空白处松手：想法保持待办，没有任何派发") {
            let frame = try await capture(showing: blank)
            let start = try row(frame, blank).center
            let geo = geometry(frame)
            try await session.drag(local: start, to: CGPoint(x: geo.width - 90, y: geo.height / 2 + 40), on: frame)
            try await Task.sleep(nanoseconds: 900_000_000)
            let state = try idea(blank)
            guard state.state == "todo", state.taskRef.isEmpty else { throw E2EError.product("落空的想法不应被派发：\(state)") }
            return "仍是待办"
        }

        try await session.step("拖动中按 Esc：想法保持待办") {
            let frame = try await capture(showing: cancelled)
            let start = try row(frame, cancelled).center
            try await session.dragCancellingWithEscape(local: start, to: try cardTarget(frame), on: frame)
            try await Task.sleep(nanoseconds: 900_000_000)
            let state = try idea(cancelled)
            guard state.state == "todo", state.taskRef.isEmpty else { throw E2EError.product("Esc 取消后想法不应被派发：\(state)") }
            return "仍是待办"
        }

        try await session.step("拖到智能体卡片：流星从左抽屉直射卡片，底部指令栏没有飞行") {
            let frame = try await capture(showing: first)
            let geo = geometry(frame)
            let start = try row(frame, first).center
            let target = try cardTarget(frame)
            let size = frame.windowFrame.size
            let baseline = try await session.snapshot()
            let samples = try await session.dragSampling(local: start, to: target, on: frame, afterMs: [60, 160, 260, 380, 520, 700])
            // 抽屉右侧到落点之间只有流星会经过（抽屉里的列表重排不算）。
            let corridor = CGRect(x: geo.drawer + 4, y: min(start.y, target.y) - 70, width: max(40, target.x - geo.drawer - 4), height: abs(target.y - start.y) + 140)
            // 指令栏本身（底部 52px 高、距底 22px）；避开卡片，卡片落点的高亮不算。
            let bottom = CGRect(x: geo.width / 2 - 250, y: geo.height - 76, width: 500, height: 44)
            let corridorChanges = samples.map { changedPixels(baseline, $0, in: corridor, windowSize: size) }
            let bottomChanges = samples.map { changedPixels(baseline, $0, in: bottom, windowSize: size) }
            for (index, image) in samples.enumerated() { try session.saveSnapshot(image, "03-flight-\(index).png") }
            // 落点必须是卡片中心：单卡宽 406.7、左上角 (抽屉宽+22, 72)、高 = 窗口高 − 171。
            let cardCenter = CGPoint(x: geo.drawer + 22 + 203.3, y: 72 + (geo.height - 171) / 2)
            let hitArea = CGRect(x: cardCenter.x - 70, y: cardCenter.y - 70, width: 140, height: 140)
            let missArea = hitArea.offsetBy(dx: 0, dy: 230) // 卡片内偏下的对照区域
            let hits = samples.map { changedPixels(baseline, $0, in: hitArea, windowSize: size) }
            let misses = samples.map { changedPixels(baseline, $0, in: missArea, windowSize: size) }
            guard (hits.max() ?? 0) >= 150, (hits.max() ?? 0) > 2 * (misses.max() ?? 0) else {
                throw E2EError.product("撞击闪光没有出现在卡片中央（中心区域 \(hits)，对照区域 \(misses)）")
            }
            guard (corridorChanges.max() ?? 0) >= 40 else {
                throw E2EError.product("松手后没有看到流星穿过抽屉右侧到卡片之间的区域：\(corridorChanges)")
            }
            // 底部区域相对松手前有一块固定差异（焦点、悬停等静态变化），不是动画：看取样之间的波动。
            let bottomSwing = (bottomChanges.max() ?? 0) - (bottomChanges.min() ?? 0)
            guard bottomSwing <= 150 else {
                throw E2EError.product("底部指令栏出现了飞行（想法的流星不应从底部起飞）：各取样 \(bottomChanges)，波动 \(bottomSwing)")
            }
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let assigned = try idea(first)
            guard assigned.state == "assigned", !assigned.taskRef.isEmpty, !assigned.taskRef.hasPrefix("idea:") else {
                throw E2EError.product("想法没有变成已指派或缺少真实任务引用：\(assigned)")
            }
            try session.save(try await session.capture(), "04-assigned.png")
            return "流星 \(corridorChanges)，中心 \(hits)，对照 \(misses)，底部 \(bottomChanges)，\(assigned.taskRef)"
        }

        try await session.step("拖到「新建智能体来完成」：想法被派发，卡片数量增加") {
            let frame = try await capture(showing: fresh)
            let geo = geometry(frame)
            let start = try row(frame, fresh).center
            let zone = CGPoint(x: geo.drawer + (geo.width - 14 - geo.drawer) / 2, y: geo.height - 126)
            try await session.drag(local: start, to: zone, on: frame)
            try await Task.sleep(nanoseconds: 2_500_000_000)
            let state = try idea(fresh)
            try session.save(try await session.capture(), "05-new-agent.png")
            guard state.state == "assigned", !state.taskRef.isEmpty else {
                throw E2EError.product("拖到新建区后想法应已指派：\(state)")
            }
            return "已指派 \(state.taskRef)"
        }

        try await session.step("点「指派」走菜单：选智能体后想法被派发") {
            var frame = try await capture(showing: viaMenu)
            let geo = geometry(frame)
            let line = try row(frame, viaMenu)
            let buttons = frame.texts.filter { $0.value.contains("指派") && $0.center.x < geo.drawer && abs($0.center.y - line.center.y) < 40 && !$0.value.contains("已指派") }
            guard let assignButton = buttons.max(by: { $0.center.x < $1.center.x }) else {
                throw E2EError.product("找不到该行的「指派」按钮：\(frame.summary)")
            }
            // OCR 常把「指派 ▾」与右侧的「×」读成一个框，框中心会落在两个按钮之间；按钮位置固定在抽屉右侧，横向取按钮自己的位置。
            try await session.click(local: CGPoint(x: geo.drawer - 97, y: line.center.y + 11), on: frame)
            frame = try await session.wait(Expectation(allOf: ["交给谁来做"], timeout: 5))
            try session.save(frame, "06-menu.png")
            guard let item = frame.texts.filter({ $0.value.contains("E2E Project") && $0.center.x < geo.drawer && $0.center.y > line.center.y }).max(by: { $0.center.y < $1.center.y }) else {
                throw E2EError.product("菜单里找不到智能体选项：\(frame.summary)")
            }
            try await session.click(local: item.center, on: frame)
            try await Task.sleep(nanoseconds: 1_800_000_000)
            let state = try idea(viaMenu)
            guard state.state == "assigned", !state.taskRef.isEmpty else {
                throw E2EError.product("菜单指派后想法应已指派：\(state)")
            }
            return "已指派 \(state.taskRef)"
        }

        try await session.step("收尾：落空与取消的想法始终是待办，且各想法互不串扰") {
            let all = try Catalog.ideas(database: try session.databaseURL())
            for body in [second, blank, cancelled, typed] {
                guard let found = all.first(where: { $0.body == body }), found.state == "todo" else {
                    throw E2EError.product("「\(body)」应保持待办：\(all)")
                }
            }
            let refs = all.filter { !$0.taskRef.isEmpty }.map(\.taskRef)
            guard Set(refs).count == refs.count else { throw E2EError.product("不同想法不应共用任务引用：\(refs)") }
            return "\(all.map { "\($0.body)=\($0.state)" })"
        }
    }
}

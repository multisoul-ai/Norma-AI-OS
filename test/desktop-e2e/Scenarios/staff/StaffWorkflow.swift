import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 真实 App + 生产 CLI/ACP gate/hook/PTy；脚本仅替代外部模型和 Agent，不写入 TaskStore。
@MainActor
public enum StaffWorkflow {
    public nonisolated static let name = "staff-workflow"
    public nonisolated static let englishName = "staff-workflow-en"

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
        try Catalog.seedWorkspace(database: database, name: "Staff M1")
        try Catalog.suppressFeatureGuide(database: database)
        if english { try VoiceConfigurationFixture.theme(database: database, id: "sapphire-night") }
        project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Staff M1")
        try session.launch(language: language)

        try await session.step("进入项目并准备一个可派活的 Agent 名称") {
            try await session.visualClick("点击 Staff M1 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击右上角侧栏图标，展开原有右侧内容抽屉")
            try await session.visualClick("点击底部命令输入框")
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("真实 Agent 启动") { try StaffFixture.records(root, "starts.jsonl").count == 1 }
            _ = try await session.visualCheck([VisualCriterion(id: "seed", text: "E2E Project 画布有一张 Claude 卡片，显示 STAFF WORKER READY；顶部有幕僚或 Staff 按钮，右侧内容抽屉可见。若 Agent 仍启动，判 wait。")])
            try await session.visualClick("点击顶部幕僚或 Staff 按钮进入幕僚模式")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "empty", text: "按已选双栏原型：顶部只有一套工作空间选择器和画布/Canvas、幕僚/Staff导航，幕僚选中；无第二行Norma标题栏或重复统计页脚。左栏约280px，新任务入口使用柔和蓝色填充，项目筛选位于左栏，搜索图标与筛选紧凑；中央圆角对话区和输入框清晰，右栏按需显示。E2E Project名称可见，不是临时UUID。"),
                VisualCriterion(id: "locale", text: english ? "界面控制文案为英文，包含 New task、Give Norma a task；用户项目名称不翻译。" : "界面控制文案为中文，包含新任务、交给 Norma 一件事。")
            ]), "01-empty.png")
            return "空态三栏和双语控制可见"
        }

        try await session.step("新建任务，经真实权限闸门生成提议") {
            try await session.visualClick("点击中央任务对话底部的消息输入框，不是左侧搜索框")
            try await session.type("STAFF-M1 请生成验收文件")
            try await session.shortcut(key: 36, modifiers: [])
            try await session.type("保留这一行补充要求")
            _ = try await session.visualCheck([VisualCriterion(id: "multiline", text: "中央输入框保留两行尚未发送的文字：STAFF-M1 请生成验收文件和保留这一行补充要求；仍是新任务空态，普通 Enter 没有发出任务；输入区有接收者、圆形上箭头和 Cmd+Enter 发送提示。")])
            guard try StaffFixture.tasks(root).isEmpty else { throw E2EError.product("普通 Enter 意外发送了任务") }
            try await session.shortcut(key: 36, modifiers: [0x37])
            try await waitFor("提议保存") { try first(root)["proposals"] as? [[String: Any]] != nil && !(try first(root)["proposals"] as? [[String: Any]] ?? []).isEmpty }
            // 对话为滚动视口，先核对首条原话，再回到底部核对完整提议；不要求整段历史同屏。
            try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: 3000)
            try session.save(try await session.visualCheck([VisualCriterion(id: "user-message", text: "中央首条用户消息右对齐圆角气泡，完整包含 STAFF-M1 请生成验收文件 与 保留这一行补充要求 两行原话；不与固定标题或Worker栏重叠。")]), "02-user.png")
            try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: -3000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "proposal", text: "左栏出现需要你/Needs you 状态的 STAFF-M1 任务；中央有Norma 提议实现验收文件、目标 Agent、理由和完整任务内容，以及可点击的采用并派发/Adopt and dispatch 按钮。"),
                VisualCriterion(id: "markdown", text: "Norma正文的STAFF PLAN为粗体，采用后开始与产物为两条列表，STAFF-RESULT.md显示为行内代码；不能直接显示双星号和反引号标记。"),
                VisualCriterion(id: "layout", text: "Norma 有独立头像与名字；提议为圆角卡片，有需要你决定标记及紧凑采用/编辑按钮。任务标题约22px，阶段选择紧凑；输入区为圆角多行框与圆形发送键。任务标题、提议和输入框没有重叠或横向裁切，右侧文件抽屉仍在。")
            ]), "02-proposal.png")
            let rows = try first(root)["messages"] as? [[String: Any]] ?? []
            let userText = rows.compactMap { ($0["User"] as? [String: Any])?["text"] as? String }.first
            guard userText == "STAFF-M1 请生成验收文件\n保留这一行补充要求" else { throw E2EError.product("发送未原样保留两行消息：\(userText ?? "nil")") }
            let rejected = try StaffFixture.records(root, "commands.jsonl").contains { row in
                (row["argv"] as? [String] ?? []).contains("TASK MUST NOT SPEAK") && row["allowed"] as? Bool == false
            }
            guard rejected else { throw E2EError.product("任务会话播报未被权限闸门拒绝") }
            guard try StaffFixture.records(root, "received.jsonl").isEmpty else { throw E2EError.product("用户采用前已派活") }
            return "提议落盘；采用前无 Worker 输入；Task report 被拒绝"
        }

        try await session.step("hover 与采用：创建独立 Worker，结果回到同一任务") {
            try await session.visualClick("点击中央底部消息输入框")
            try await session.type("EDIT-DRAFT 保留已有补充")
            try await session.visualClick("点击提议卡的修改后发送/Edit and send按钮")
            _ = try await session.visualCheck([VisualCriterion(id: "edit-draft", text: "输入框同时保留 EDIT-DRAFT 保留已有补充 和提议原文 STAFF-M1-WORK，提议没有覆盖原有草稿，也没有自动发送。")])
            try await session.selectAll(); try await session.deleteSelection()
            let target = try await session.visualLocate("定位提议卡的采用并派发/Adopt and dispatch 按钮中心")
            try await session.hover(local: target.point, on: target.frame)
            try session.save(try await session.visualCheck([VisualCriterion(id: "hover", text: "提议采用按钮悬停反馈清楚可辨，文字与边框清晰，布局未跳动。")]), "03-hover.png")
            let adopt = try await session.visualLocate("定位提议卡的采用并派发/Adopt and dispatch 按钮，准备按下")
            try await session.press(local: adopt.point, on: adopt.frame) {
                try session.save(try await session.visualCheck([VisualCriterion(id: "pressed", text: "采用按钮按下状态比悬停状态更明显，文字可读，位置和尺寸未改变；尚未显示已派发。")]), "03-pressed.png")
            }
            try await waitFor("Worker 返回可信结果") {
                let task = try first(root)
                return task["phase"] as? String == "Merging" && messages(task).contains("STAFF REVIEW")
            }
            let task = try first(root)
            let workers = task["workers"] as? [[String: Any]] ?? []
            let received = try StaffFixture.records(root, "received.jsonl")
            let starts = try StaffFixture.records(root, "starts.jsonl")
            guard workers.count == 1, received.count == 1, starts.count == 2,
                  workers[0]["card_id"] as? Int == received[0]["node"] as? Int,
                  workers[0]["dispatch_id"] as? String == received[0]["dispatch"] as? String,
                  received[0]["node"] as? Int != starts[0]["node"] as? Int,
                  (received[0]["task"] as? String ?? "").contains("STAFF-M1-WORK") else {
                throw E2EError.product("新卡身份/dispatch/恰好一次输入不符合：\(workers), \(received), \(starts)")
            }
            // 从最新结果向上滚动一个提议卡的高度，检查已采用的禁用按钮，再回到结果。
            try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: 250)
            _ = try await session.visualCheck([VisualCriterion(id: "dispatched", text: "历史提议中的采用按钮已变成 已派发/Dispatched 禁用状态，不能再次派活；Worker卡条显示结果状态。")])
            try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: -3000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "result", text: "任务处于待验收/In review，中央有 Worker 结果 STAFF RESULT 与 Norma 的 STAFF REVIEW 回复；标题下有独立 Worker 卡条，卡片约230px宽，显示头像、真实 Agent 名称、引擎、角色和结果状态，能定位卡片。消息区在 Worker 卡条下方；若消息在上方，允许滚动查看后重试。")
            ]), "04-result.png")
            return "同名 Agent 新卡；dispatch 精确匹配；恰好一次送达；自动推进待验收"
        }

        try await session.step("搜索按标题与 Agent 名称过滤，筛选不清空当前任务") {
            try await session.visualClick("点击中央底部消息输入框")
            try await session.type("DRAFT 保留草稿第一行\nDRAFT 保留草稿第二行")
            try await session.visualClick("点击左侧新任务按钮右侧的搜索放大镜图标")
            try await session.visualClick("点击展开的搜索任务/Search tasks输入框")
            try await session.type("no-such-staff-task")
            _ = try await session.visualCheck([VisualCriterion(id: "no-match", text: "左侧搜索无匹配结果，任务行隐藏；中央原任务及Worker卡条仍在，输入框仍保留 DRAFT 保留草稿第一行和 DRAFT 保留草稿第二行 两行原文。")])
            try await session.selectAll()
            let workers = try first(root)["workers"] as? [[String: Any]] ?? []
            guard let agent = workers.first?["agent"] as? String else { throw E2EError.product("缺少可搜索的Worker名称") }
            try await session.type(agent)
            _ = try await session.visualCheck([VisualCriterion(id: "agent-search", text: "左侧搜索框显示Worker真实Agent名称，STAFF-M1任务行仍匹配；行里有Agent与项目，未重复堆叠完成计数和头像，任务行紧凑可读。")])
            try await session.selectAll(); try await session.type("STAFF-M1")
            _ = try await session.visualCheck([VisualCriterion(id: "title-search", text: "按STAFF-M1标题搜索仍能看到同一任务。")])
            try await session.selectAll(); try await session.deleteSelection()
            try await session.visualClick("点击左侧筛选条的进行中/Running按钮")
            _ = try await session.visualCheck([VisualCriterion(id: "filter", text: "进行中/Running筛选为选中状态，待验收任务被过滤，中央已选任务保留。")])
            try await session.visualClick("点击左侧筛选条的全部/All按钮")
            try session.save(try await session.visualCheck([VisualCriterion(id: "restored", text: "全部/All恢复选中，左侧待验收任务行恢复，中央内容保持，输入框仍含两行DRAFT草稿。")]), "04-search.png")
            return "无匹配、Agent与标题搜索、筛选和恢复均有效"
        }

        try await session.step("右栏拖宽与窄窗仍保留对话") {
            let edge = try await session.visualLocate("定位中央幕僚面板与右侧内容抽屉之间可拖动的竖分隔线，在高度300处")
            try await session.drag(local: edge.point, to: CGPoint(x: 120, y: edge.point.y), on: edge.frame)
            _ = try await session.visualCheck([VisualCriterion(id: "wide-drawer", text: "右栏拖到上限后，左任务栏仍完整，中央仍至少约320px宽；任务标题、Worker卡、输入框和发送按钮可见且不重叠。两行DRAFT草稿仍在。")])
            try await session.resize(width: 900, height: 800)
            try session.save(try await session.visualCheck([VisualCriterion(id: "narrow", text: "900px窄窗中，左任务列表约280px，中央对话与输入区仍可用、发送按钮可见；右抽屉没有遮住或挤没对话，顶部模式与全屏按钮仍可见；两行DRAFT草稿保留。")]), "04-narrow.png")
            try await session.resize(width: 760, height: 800)
            _ = try await session.visualCheck([VisualCriterion(id: "breakpoint", text: "760px窗口右抽屉隐藏，保留左任务列表与中央可用对话，输入和发送按钮清楚可见。")])
            try await session.resize(width: 600, height: 800)
            _ = try await session.visualCheck([VisualCriterion(id: "compact", text: "600px窗口按原型隐藏左任务列表与右抽屉，任务对话占主要宽度；Worker卡、两行DRAFT输入和发送按钮完整可用，没有遮挡或横向溢出。")])
            try await session.visualClick("点击顶部任务列表图标（三条横线），在600px窄窗展开任务列表")
            _ = try await session.visualCheck([VisualCriterion(id: "compact-tasks", text: "窄窗出现任务列表浮层，项目筛选、搜索、STAFF-M1任务均可访问；只有一套顶部导航，没有重复标题。")])
            try await session.visualClick("点击顶部全屏图标，从任务浮层进入全屏")
            _ = try await session.visualCheck([VisualCriterion(id: "compact-full", text: "已进入全屏，任务浮层关闭；顶部退出全屏按钮可见，任务对话和两行DRAFT草稿仍在。")])
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.visualCheck([VisualCriterion(id: "compact-return", text: "单次Escape已退出全屏并回到窄窗口；任务浮层关闭，当前任务与两行DRAFT草稿保留，发送按钮可用。")])
            try await session.resize(width: 1280, height: 800)
            let restore = try await session.visualLocate("定位幕僚面板与右内容抽屉之间的竖分隔线，高度300处，准备恢复右栏宽度")
            try await session.drag(local: restore.point, to: CGPoint(x: 900, y: restore.point.y), on: restore.frame)
            return "极限拖宽和900px窄窗仍能使用任务对话，恢复380px右栏"
        }

        try await session.step("右侧复用文件抽屉打开 Worker 产物") {
            try await session.visualClick("点击右侧内容抽屉中的文件/Files工具；不要点击任务里的路径文字")
            try await session.visualClick("点击右侧文件树的所有文件/All files分区标题，展开默认折叠的目录树")
            try await session.visualClick("点击右侧已展开文件树中的 STAFF-RESULT.md 文件")
            try session.save(try await session.visualCheck([VisualCriterion(id: "artifact", text: "右侧文件标签与正文打开 STAFF-RESULT.md，正文包含 STAFF RESULT；左侧任务列表和中央任务对话仍在同一个幕僚页面。")]), "05-artifact.png")
            return "原文件抽屉可读取真实 Worker 文件"
        }

        try await session.step("自动完成、新消息重开及手动阶段保护") {
            try await send("CONFIRM 验收通过", session)
            try await waitFor("Norma 自动完成") { try first(root)["phase"] as? String == "Done" && first(root)["phase_manual"] as? Bool == false }
            try await send("REOPEN 补充一个要求", session)
            try await waitFor("完成任务重开") { try first(root)["phase"] as? String == "Active" && messages(first(root)).contains("STAFF REOPEN") }
            try await session.visualClick("点击中央任务标题右侧的进行中/Active阶段下拉选择器")
            try await session.visualClick("点击阶段菜单中的进行中/Active选项，以用户身份固定这个阶段")
            try await send("MANUAL 保持我的阶段", session)
            try await waitFor("手动阶段被保护") { try first(root)["phase_manual"] as? Bool == true && messages(first(root)).contains("STAFF MANUAL") }
            guard try first(root)["phase"] as? String == "Active" else { throw E2EError.product("Norma 覆盖了用户阶段") }
            try session.save(try await session.visualCheck([VisualCriterion(id: "manual", text: "任务阶段仍为进行中/Active；有你设置的阶段/Phase set by you说明。对话已包含重开和 STAFF MANUAL 回复，没有错误提示。")]), "06-manual.png")
            return "自动 Done、Done→Active 重开、用户阶段不被自动 Merging 覆盖"
        }

        try await session.step("全屏、Escape 恢复与画布切换") {
            try await session.visualClick("点击幕僚标题栏中的全屏/Full screen按钮")
            try session.save(try await session.visualCheck([VisualCriterion(id: "full", text: "幕僚任务界面填满窗口，保留一套顶部工作空间/模式导航和退出全屏按钮；任务列表收起以放大对话，任务内容与右侧文件仍在；顶部控件不与macOS窗口按钮重叠。")]), "07-fullscreen.png")
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.visualCheck([VisualCriterion(id: "escape", text: "Escape 已退出幕僚全屏，任务列表恢复，任务和打开的文件保留。")])
            try await session.visualClick("点击幕僚标题栏中的画布/Canvas按钮返回画布")
            try session.save(try await session.visualCheck([VisualCriterion(id: "canvas", text: "原画布恢复，有两张独立 Claude 卡片；右侧 STAFF-RESULT.md 仍打开，没有重复创建第三张卡。")]), "08-canvas.png")
            guard try StaffFixture.records(root, "starts.jsonl").count == 2 else { throw E2EError.product("切换页面重启了 Worker") }
            return "全屏与模式切换保留原 Runtime 和文件标签"
        }
    }

    private static func first(_ root: URL) throws -> [String: Any] { try StaffFixture.tasks(root).first ?? [:] }
    private static func messages(_ task: [String: Any]) -> String {
        String(data: (try? JSONSerialization.data(withJSONObject: task["messages"] ?? [])) ?? Data(), encoding: .utf8) ?? ""
    }
    private static func send(_ text: String, _ session: Session) async throws {
        try await session.visualClick("点击中央任务对话底部的消息输入框；不要点击右侧抽屉的搜索框")
        try await session.selectAll(); try await session.type(text)
        try await session.shortcut(key: 36, modifiers: [0x37])
    }
    private static func waitFor(_ description: String, _ check: () throws -> Bool) async throws {
        // 60s covers two local subprocess handshakes and hook completion debounce; no network model.
        let deadline = Date().addingTimeInterval(60)
        while Date() < deadline {
            if try check() { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("等待超时：\(description)")
    }
}

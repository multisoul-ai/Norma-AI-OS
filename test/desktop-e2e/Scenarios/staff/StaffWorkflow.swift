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
                VisualCriterion(id: "empty", text: "左栏有新任务/New task，中央显示输入新任务的空态与发送按钮，右栏复用原内容抽屉；没有卡片叠在任务界面上。"),
                VisualCriterion(id: "locale", text: english ? "界面控制文案为英文，包含 Tasks、New task、Send；用户项目名称不翻译。" : "界面控制文案为中文，包含任务、新任务、发送。")
            ]), "01-empty.png")
            return "空态三栏和双语控制可见"
        }

        try await session.step("新建任务，经真实权限闸门生成提议") {
            try await send("STAFF-M1 请生成验收文件", session)
            try await waitFor("提议保存") { try first(root)["proposals"] as? [[String: Any]] != nil && !(try first(root)["proposals"] as? [[String: Any]] ?? []).isEmpty }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "proposal", text: "左栏需要你/Needs you 分组出现 STAFF-M1 任务；中央有用户原话、Norma 提议实现验收文件、目标 Agent、理由和完整任务内容，以及可点击的采用并派发/Adopt and dispatch 按钮。"),
                VisualCriterion(id: "layout", text: "任务标题、phase 选择器、提议和发送框没有重叠或横向裁切；右侧文件抽屉仍在。")
            ]), "02-proposal.png")
            let rejected = try StaffFixture.records(root, "commands.jsonl").contains { row in
                (row["argv"] as? [String] ?? []).contains("TASK MUST NOT SPEAK") && row["allowed"] as? Bool == false
            }
            guard rejected else { throw E2EError.product("任务会话播报未被权限闸门拒绝") }
            guard try StaffFixture.records(root, "received.jsonl").isEmpty else { throw E2EError.product("用户采用前已派活") }
            return "提议落盘；采用前无 Worker 输入；Task report 被拒绝"
        }

        try await session.step("hover 与采用：创建独立 Worker，结果回到同一任务") {
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
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "result", text: "任务处于待验收/In review，中央有 Worker 结果 STAFF RESULT 与 Norma 的 STAFF REVIEW 回复；采用按钮显示已派发/Dispatched 且禁用，Worker 标签可定位。若消息在上方，允许滚动查看后重试。")
            ]), "04-result.png")
            return "同名 Agent 新卡；dispatch 精确匹配；恰好一次送达；自动推进待验收"
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
            try session.save(try await session.visualCheck([VisualCriterion(id: "full", text: "幕僚任务界面填满窗口，顶部原工作空间工具栏收起，出现退出全屏/Exit full screen按钮，任务内容与右侧文件仍在；左上任务/Tasks标题完整可读，不与 macOS 红黄绿窗口按钮重叠。")]), "07-fullscreen.png")
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.visualCheck([VisualCriterion(id: "escape", text: "Escape 已退出幕僚全屏，原工作空间顶栏恢复，任务和打开的文件保留。")])
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
        try await session.shortcut(key: 36, modifiers: [])
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

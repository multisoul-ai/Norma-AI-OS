import AppKit
import E2EKit
import Fixtures
import Foundation

/// 真实App、生产Hook和原生PTY权限菜单；脚本Agent仅记录决定，不执行命令。
/// 输入经debug控制通道，前台PID断言仅证明产品处理动作没有主动激活App。
@MainActor
public enum IslandApprovals {
    public nonisolated static let name = "island-approvals"
    public nonisolated static let rememberedName = "island-remembered-approval"
    public nonisolated static let fallbackName = "island-native-approval-fallback"

    public static func run(_ session: Session, remembered: Bool, fallback: Bool = false) async throws {
        let originalFront = NSWorkspace.shared.frontmostApplication
        let fixture = try BrainApprovalFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR", "NORMA_E2E_NO_ACTIVATE"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            if let output = session.output {
                for name in ["approval-decisions.jsonl", "approval-hook.log"] {
                    if let data = try? Data(contentsOf: fixture.brain.appendingPathComponent(name)) {
                        try? data.write(to: output.appendingPathComponent(name))
                    }
                }
            }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            for path in [fixture.shell, fixture.brain, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", fixture.shell.path, 1)
        setenv("NORMA_BRAIN_DIR", fixture.brain.path, 1)
        setenv("CLAUDE_CONFIG_DIR", fixture.brain.appendingPathComponent("claude-config").path, 1)
        try await session.step("建立隔离审批项目并启动真实Agent卡片") {
            try session.launch(language: "zh-Hans")
            _ = try await session.capturePixels()
            session.stop()
            guard let root = session.dataRoot else { throw E2EError.env("缺少隔离数据目录") }
            let db = try session.databaseURL()
            try Catalog.seedWorkspace(database: db, name: "Island approvals")
            try Catalog.suppressFeatureGuide(database: db)
            project = try CanvasProjectFixture.install(dataRoot: root, workspaceName: "Island approvals")
            try session.launch(language: "zh-Hans")
            // The native approval contract requires the complete CLI menu, including its footer.
            // A separate narrow-card case below verifies the safe fallback instead of losing it.
            if !fallback { try await session.resize(width: 1440, height: 840) }
            try await session.visualClick("点击名为Island approvals的工作空间卡片")
            try await session.visualClick("点击E2E Project项目卡片进入画布")
            try await session.visualClick("点击画布底部的命令输入框")
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "agent-ready", text: "画布中有Claude智能体卡片，终端显示Approval fixture ready；没有错误提示")
            ]), "01-agent-ready.png")
            return "脚本Agent通过真实PTY启动，尚未发出审批"
        }
        try await session.step("在独立岛处理第一条审批并核对原生消费记录") {
            setenv("NORMA_E2E_NO_ACTIVATE", "1", 1)
            let windows = try await session.nativeWindows()
            guard let island = windows.first(where: { $0.width >= 300 && $0.width < 700 && $0.height <= 60 }) else {
                throw E2EError.product("找不到独立岛窗口")
            }
            try await session.selectNativeWindow(island.number)
            originalFront?.activate(options: [])
            try await Task.sleep(nanoseconds: 300_000_000)
            let front = NSWorkspace.shared.frontmostApplication?.processIdentifier
            guard front != nil, front != session.app?.processIdentifier else { throw E2EError.env("无法建立另一App在前台的前置条件") }
            try Data().write(to: fixture.brain.appendingPathComponent("approval-1"))
            for _ in 0..<150 {
                if try await session.nativeWindows().contains(where: { $0.number == island.number && $0.height > 60 }) { break }
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            if fallback {
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "native-fallback", text: "岛弹出审批提醒，说明需在原智能体卡片中处理，有定位按钮；没有可点击的允许一次、总是允许或拒绝按钮")
                ]), "02-native-fallback.png")
                guard try records(fixture.brain).isEmpty else { throw E2EError.product("未知原生菜单被错误放行") }
                try await session.visualClick("点击定位按钮前往原智能体卡片")
                try await session.selectNativeWindow(nil)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "native-menu", text: "主工作空间显示原Claude卡片，终端内有Bash command和Do you want to proceed权限菜单；没有新建第二张卡片")
                ]), "03-native-menu.png")
                return "未知原生菜单保持未审批，并能定位到原卡处理"
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "approval", text: "与原型一致：提醒顶部右侧显示Island approvals工作空间名；黑色胶囊里为深灰圆角审批卡片，头像/标题/来源两级层次，橙色审批标记；黑色代码框中完整命令cargo test -p norma --test brain_decision_copy可读；底部左边全部待办1，右边拒绝与白底允许一次，没有总是允许，没有运行空态")
            ]), "02-approval.png")
            guard try records(fixture.brain).isEmpty else { throw E2EError.product("用户操作前已放行") }
            if remembered {
                try await session.visualClick("点击全部待办进入完整待办列表")
                _ = try await session.visualCheck([VisualCriterion(id: "expanded-actions", text: "需要你处理页签选中，审批卡片显示定位、拒绝、总是允许、允许一次；仍显示完整命令")])
            }
            try await session.visualClick(remembered ? "点击总是允许按钮" : "点击允许一次按钮")
            try await waitForDecision(fixture.brain, cycle: 1, behavior: "allow")
            guard NSWorkspace.shared.frontmostApplication?.processIdentifier == front else { throw E2EError.product("审批使前台App发生变化") }
            return "原生权限菜单恰好消费一次allow；处理期间前台PID未变"
        }
        if fallback { return }
        try await session.step(remembered ? "相同操作的新请求复用精确授权" : "新请求仍需审批并可拒绝") {
            if remembered {
                // 已从“全部待办”进入Expanded，确认后面板保持展开。
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "remembered", text: "岛显示已经记住这项精确操作的成功提示，没有授权保存失败的提示")
                ]), "03-remembered.png")
            }
            try Data().write(to: fixture.brain.appendingPathComponent("approval-2"))
            if !remembered {
                try await session.visualClick("等待第二条审批卡片出现，点击拒绝按钮")
            }
            try await waitForDecision(fixture.brain, cycle: 2, behavior: remembered ? "allow" : "deny")
            guard try records(fixture.brain).count == 2 else { throw E2EError.product("重复消费了审批") }
            return remembered ? "新请求ID的相同操作自动允许，原生消费总计两次" : "允许一次没有扩大授权，第二请求被明确拒绝"
        }
    }

    private static func records(_ root: URL) throws -> [[String: Any]] {
        let path = root.appendingPathComponent("approval-decisions.jsonl")
        guard FileManager.default.fileExists(atPath: path.path) else { return [] }
        return try String(contentsOf: path, encoding: .utf8).split(separator: "\n").map {
            guard let row = try JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] else { throw E2EError.product("无效原生消费记录") }
            return row
        }
    }
    private static func waitForDecision(_ root: URL, cycle: Int, behavior: String) async throws {
        for _ in 0..<100 {
            let matches = try records(root).filter { $0["cycle"] as? Int == cycle }
            if let row = matches.first {
                guard matches.count == 1, row["tool_use_id"] as? String == "approval-ui-tool-\(cycle)", row["behavior"] as? String == behavior else {
                    throw E2EError.product("决定重复、错请求或内容错误：\(matches)")
                }
                return
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("原生CLI未消费第\(cycle)条审批")
    }
}

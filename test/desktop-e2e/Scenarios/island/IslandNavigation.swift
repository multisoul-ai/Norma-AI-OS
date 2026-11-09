import Darwin
import E2EKit
import Fixtures
import Foundation

/// 跨空间定位与关闭原生主窗口后重开；用真实脚本Agent进程身份证明没有重启Agent。
/// 手机服务使用已有 e2e-test-support 的回环临时端口隔离，实际启动服务而非注入 enabled。
@MainActor
public enum IslandNavigation {
    public nonisolated static let name = "island-navigation"

    public static func run(_ session: Session) async throws {
        let fixture = try IslandNavigationFixture.install()
        let keys = ["ZDOTDIR", "CLAUDE_CONFIG_DIR", "NORMA_MOBILE_L3_LOOPBACK", "NORMA_E2E_INPUT"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        var project: URL?
        defer {
            session.stop()
            if let output = session.output, let data = try? Data(contentsOf: fixture.appendingPathComponent("starts.jsonl")) {
                try? data.write(to: output.appendingPathComponent("agent-starts.jsonl"))
            }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            for path in [fixture, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", fixture.path, 1)
        setenv("CLAUDE_CONFIG_DIR", fixture.appendingPathComponent("claude-config").path, 1)
        setenv("NORMA_MOBILE_L3_LOOPBACK", "1", 1)
        unsetenv("NORMA_E2E_INPUT")
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let root = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Island navigation")
        try Catalog.seedWorkspace(database: database, name: "Other workspace")
        try Catalog.suppressFeatureGuide(database: database)
        project = try CanvasProjectFixture.install(dataRoot: root, workspaceName: "Island navigation")
        try session.launch(language: "zh-Hans")
        try await session.step("启动真实PTY任务并确认保留窗口的手机服务已开启") {
            try await session.visualClick("点击Island navigation工作空间卡片")
            try await session.visualClick("点击E2E Project项目进入画布")
            try await session.visualClick("点击底部命令输入框")
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "live", text: "E2E Project画布显示一张Claude卡片，终端有ISLAND LIVE及PID和NODE；卡片为运行中而非Done或错误")
            ]), "01-live.png")
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await session.scroll(xFraction: 0.25, yFraction: 0.5, deltaY: -900)
            try await session.visualClick("点击设置导航中的手机连接")
            _ = try await session.visualCheck([
                VisualCriterion(id: "service", text: "手机连接已开启并显示127.0.0.1的回环端口；没有Address already in use错误。若显示关闭，则隔离端口前置条件不成立")
            ])
            try await session.shortcut(key: 53, modifiers: [])
            return "真实服务已开启；关窗后依法保留原项目"
        }
        let identity = try starts(fixture)
        guard identity.count == 1, let pid = identity[0]["pid"] as? Int32 else { throw E2EError.product("Agent启动身份不唯一") }
        let windows = try await session.nativeWindows()
        guard let main = windows.first(where: { $0.width > 900 }),
              let island = windows.first(where: { $0.width >= 300 && $0.width < 700 }) else { throw E2EError.product("缺少主窗口或岛") }
        try await session.step("从另一工作空间定位到原运行卡片") {
            try await session.shortcut(key: 126, modifiers: [0x37, 0x38])
            try await session.visualClick("点击Other workspace切换到另一工作空间")
            _ = try await session.visualCheck([VisualCriterion(id: "other", text: "主窗口顶部为Other workspace，不显示ISLAND LIVE卡片")])
            try await session.selectNativeWindow(island.number)
            try await locate(session, image: "02-cross-workspace.png")
            try await session.selectNativeWindow(main.number)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "located", text: "已回到Island navigation及E2E Project，镜头聚焦原Claude卡片，ISLAND LIVE终端可见，没有重复卡片")
            ]), "03-located.png")
            try requireSameProcess(fixture, pid: pid)
            return "跨空间定位保持同一Agent进程"
        }
        try await session.step("主窗口关闭后岛仍可定位并重开原项目") {
            let close = try await session.visualLocate("定位窗口左上角红色关闭按钮；不要点黄色最小化或绿色全屏")
            setenv("NORMA_E2E_INPUT", "ns", 1)
            do {
                try await session.click(local: close.point, on: close.frame)
            } catch {
                // 原生关闭在 MouseUp 内完成，因此这个窗口不可能再绘制 ACK 帧。
                // 仅在已释放鼠标、精确窗口已消失时接受；不重放输入，不吞其他传输错误。
                let reason = String(describing: error)
                guard reason.contains("phase=up"),
                      reason.contains("E2E frame barrier: window unavailable"),
                      try await session.nativeWindows().allSatisfy({ $0.number != main.number }) else {
                    throw error
                }
            }
            unsetenv("NORMA_E2E_INPUT")
            for _ in 0..<50 {
                if try await session.nativeWindows().allSatisfy({ $0.number != main.number }) { break }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            guard try await session.nativeWindows().allSatisfy({ $0.number != main.number }) else { throw E2EError.product("主窗口未真正关闭") }
            try requireSameProcess(fixture, pid: pid)
            try await session.selectNativeWindow(island.number)
            try await locate(session, image: "04-retained.png")
            for _ in 0..<100 {
                if try await session.nativeWindows().contains(where: { $0.width > 900 }) { break }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            guard let reopened = try await session.nativeWindows().first(where: { $0.width > 900 }), reopened.number != main.number else { throw E2EError.product("没有重开新的原生窗口") }
            try await session.selectNativeWindow(reopened.number)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "reopened", text: "重开的主窗口显示Island navigation / E2E Project与原ISLAND LIVE Claude卡片，运行中；没有空白项目、错误或重复Agent")
            ]), "05-reopened.png")
            try requireSameProcess(fixture, pid: pid)
            return "主窗口号改变、Agent PID不变且启动记录恰一条，原终端内容恢复"
        }
    }

    private static func locate(_ session: Session, image: String) async throws {
        let frame = try await session.capturePixels()
        if frame.windowFrame.height <= 60 { try await session.visualClick("点击黑色胶囊中央展开岛") }
        try await session.visualClick("点击运行中页签")
        try session.save(try await session.visualCheck([
            VisualCriterion(id: "running", text: "岛在Island navigation分组下显示1运行中摘要，一条运行任务有彩色头像、名字、Claude Provider、任务标题；第二行当前步骤及计时，右侧运行中状态与定位按钮；不是未加载或空态")
        ]), image)
        try await session.visualClick("点击E2E Project运行任务的定位按钮")
    }

    private static func starts(_ root: URL) throws -> [[String: Any]] {
        try String(contentsOf: root.appendingPathComponent("starts.jsonl"), encoding: .utf8).split(separator: "\n").map {
            guard let row = try JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] else { throw E2EError.product("无效Agent身份记录") }
            return row
        }
    }

    private static func requireSameProcess(_ root: URL, pid: Int32) throws {
        let records = try starts(root)
        guard records.count == 1, records[0]["pid"] as? Int32 == pid, kill(pid, 0) == 0 else { throw E2EError.product("关窗或定位重建/结束了原Agent进程") }
    }
}

import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 想法拖到智能体卡片时，真实画布上播放用户选的派发特效；设置页里改款式立刻对下一次派发生效。
/// 构造：隔离工作空间 + 项目 + 假智能体；启动前把工作空间特效写成烟花；之后在设置页实时改成闪电、星漩。
/// 预期：每次派发后画布区域都有明显动画，并按款式的时长收尾——
/// 闪电（0.76s）远短于烟花（1.9s）与星漩（1.76s）；想法都变成已指派。
@MainActor
public enum WorkspaceDispatchEffects {
    public nonisolated static let name = "workspace-dispatch-effects"
    private static let workspace = "FXRUN"
    private static let ideas = ["烟花测试想法", "闪电测试想法", "星漩测试想法"]

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }

        try await session.step("建立含项目、三条想法并预置烟花的工作空间") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "Create your first"], timeout: 25))
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
            for (offset, body) in ideas.enumerated() {
                try Catalog.seedIdea(database: database, workspaceName: workspace, body: body, position: 1024 * (offset + 1))
            }
            try Catalog.seedWorkspaceDispatchEffect(database: database, workspaceName: workspace, json: "{\"style\":\"fireworks\",\"handoff\":\"ion\"}")
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return "已写入 3 条想法与 {style:fireworks}"
        }

        try await session.step("进入项目并创建一个智能体，打开想法栏") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540), on: launch)
                _ = try await session.wait(Expectation(absent: ["搜索工作空间", "Search workspaces"], timeout: 12))
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            let entered = try await session.capture()
            if entered.contains("Feature Guide") || entered.contains("Skip Guide") {
                try await session.shortcut(key: 53, modifiers: [])
                _ = try await session.wait(Expectation(absent: ["Feature Guide", "Skip Guide"], timeout: 6))
            }
            let frame = try await session.capture()
            try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38), on: frame)
            try await session.selectAll()
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            try await Task.sleep(nanoseconds: 2_500_000_000)
            // 想法栏的标题是固定中文，这台机器的 OCR 读不了；只需要它处于打开状态，队首想法由 ⌃1 派发。
            try await session.shortcut(key: 34, modifiers: [0x37, 0x38])
            try await Task.sleep(nanoseconds: 1_000_000_000)
            return "智能体卡片已创建，想法栏只展开想法"
        }

        let spans = Box()
        try await session.step("烟花：拖拽派发后画布有动画，按烟花的时长收尾") {
            let span = try await dispatch(session, idea: ideas[0], tag: "fireworks")
            spans.fireworks = span
            return "活动跨度 \(span)ms"
        }
        try await session.step("设置页实时改成闪电，再派发：动画明显更短") {
            try await chooseStyle(session, labels: ["Lightning"])
            let span = try await dispatch(session, idea: ideas[1], tag: "lightning")
            spans.lightning = span
            guard span < spans.fireworks - 500 else {
                throw E2EError.product("闪电（0.76s）应明显短于烟花（1.9s）：闪电 \(span)ms，烟花 \(spans.fireworks)ms")
            }
            return "闪电 \(span)ms < 烟花 \(spans.fireworks)ms"
        }
        try await session.step("设置页实时改成星漩，再派发：动画又变长") {
            try await chooseStyle(session, labels: ["Vortex"])
            let span = try await dispatch(session, idea: ideas[2], tag: "vortex")
            guard span > spans.lightning + 500 else {
                throw E2EError.product("星漩（1.76s）应明显长于闪电（0.76s）：星漩 \(span)ms，闪电 \(spans.lightning)ms")
            }
            return "星漩 \(span)ms > 闪电 \(spans.lightning)ms"
        }
        try await session.step("三条想法都已指派且互不串扰") {
            let all = try Catalog.ideas(database: try session.databaseURL())
            for body in ideas {
                guard let found = all.first(where: { $0.body == body }), found.state == "assigned", !found.taskRef.isEmpty else {
                    throw E2EError.product("「\(body)」应已指派：\(all)")
                }
            }
            return "\(all.map { "\($0.body)=\($0.state)" })"
        }
    }

    private final class Box { var fireworks = 0; var lightning = 0 }

    private static func geometry(_ frame: Frame) -> (width: Double, height: Double, drawer: Double) {
        let width = Double(frame.windowFrame.width)
        return (width, Double(frame.windowFrame.height), min(438, max(366, width * 0.31)))
    }

    /// 在设置页「特效」里点一个款式后关掉设置。
    private static func chooseStyle(_ session: Session, labels: [String]) async throws {
        try await session.shortcut(key: 43, modifiers: [0x37])
        _ = try await session.wait(Expectation(anyOf: ["Communication Style", "Basic Settings"], timeout: 10))
        _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Dispatch effects"])
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
        for _ in 0..<10 {
            let frame = try await session.capture()
            if let cell = frame.pick(labels, .bottom), cell.center.x > frame.windowFrame.width * 0.3 {
                try await session.click(local: CGPoint(x: cell.center.x, y: cell.center.y - 40), on: frame)
                try await Task.sleep(nanoseconds: 600_000_000)
                try await session.shortcut(key: 53, modifiers: [])
                _ = try await session.wait(Expectation(absent: ["Basic Settings"], timeout: 6))
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -160)
        }
        throw E2EError.product("特效库找不到 \(labels)")
    }

    /// `⌃1` 把队首想法派给第 1 张卡片，随后连续取样，返回画布区域“有动画”的时间跨度（ms）。
    private static func dispatch(_ session: Session, idea body: String, tag: String) async throws -> Int {
        let frame = try await session.capture()
        let geo = geometry(frame)
        let size = frame.windowFrame.size
        let baseline = try await session.snapshot()
        // 卡片自己有运行中的边框动画：连续取几帧，把其中最大的变化当作背景噪声。
        var noise = 0
        for _ in 0..<6 {
            try await Task.sleep(nanoseconds: 200_000_000)
            noise = max(noise, changedPixels(baseline, try await session.snapshot(), in: canvasArea(geo), windowSize: size))
        }
        try await session.shortcut(key: 18, modifiers: [0x3B])
        let marks: [UInt64] = Array(stride(from: 150, through: 2700, by: 150))
        var samples: [CGImage] = []
        var times: [Int] = []
        let started = ProcessInfo.processInfo.systemUptime
        for mark in marks {
            let remaining = Double(mark) - (ProcessInfo.processInfo.systemUptime - started) * 1000
            if remaining > 0 { try await Task.sleep(nanoseconds: UInt64(remaining * 1_000_000)) }
            samples.append(try await session.snapshot())
            times.append(Int((ProcessInfo.processInfo.systemUptime - started) * 1000))
        }
        guard let settled = samples.last else { throw E2EError.env("没有取得派发动画画面") }
        // 派发会永久改变卡片里的文字；用收尾画面消除静态差异，不能把新文字算成持续动画。
        let changes = samples.map { changedPixels(settled, $0, in: canvasArea(geo), windowSize: size) }
        let finalChange = changedPixels(samples[samples.count - 2], settled, in: canvasArea(geo), windowSize: size)
        guard finalChange <= noise + 150 else {
            throw E2EError.product("\(tag)：取样结束后画布仍在变化（噪声 \(noise)，变化 \(finalChange)）")
        }
        for (index, image) in samples.enumerated() where index % 2 == 0 { try session.saveSnapshot(image, "\(tag)-\(index).png") }
        let active = changes.enumerated().filter { $0.element > noise + 150 }.map { times[$0.offset] }
        guard let first = active.first, let last = active.last, last > first else {
            throw E2EError.product("\(tag)：⌃1 后画布没有动画（想法「\(body)」，噪声 \(noise)，各取样 \(changes)）")
        }
        try await Task.sleep(nanoseconds: 800_000_000)
        return last - first
    }

    /// 抽屉右侧的画布区域，去掉标题栏与底部指令栏。
    private static func canvasArea(_ geo: (width: Double, height: Double, drawer: Double)) -> CGRect {
        CGRect(x: geo.drawer + 4, y: 84, width: geo.width - geo.drawer - 8, height: geo.height - 84 - 90)
    }
}

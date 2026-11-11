import E2EKit
import Fixtures
import Foundation

/// 真实 App 上对照灵动岛 2.0 原型。角色来自画布恢复和产品 Hook，不注入岛状态。
/// 窗口宽高是几何证据；看图清单对照 reference/*.png。没有刘海、外接屏或全屏 Space 的机器不会被写成通过。
@MainActor
public enum IslandRedesign {
    public nonisolated static let name = "island-redesign"

    public static func run(_ session: Session) async throws {
        let fixture = try IslandRedesignFixture.install()
        let keys = ["ZDOTDIR", "OPENCODE_BIN", "CURSOR_AGENT_BIN", "MULTISOUL_REDUCE_MOTION"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            if let output = session.output {
                for name in ["agent.log"] {
                    if let data = try? Data(contentsOf: fixture.appendingPathComponent(name)) {
                        try? data.write(to: output.appendingPathComponent(name))
                    }
                }
            }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            try? FileManager.default.removeItem(at: fixture)
        }
        setenv("ZDOTDIR", fixture.path, 1)
        unsetenv("OPENCODE_BIN")
        unsetenv("CURSOR_AGENT_BIN")
        unsetenv("MULTISOUL_REDUCE_MOTION")
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        guard session.dataRoot != nil else { throw E2EError.env("缺少隔离目录") }
        let database = try session.databaseURL()
        session.stop()
        try Catalog.suppressFeatureGuide(database: database)
        try await phase(session, database: database, fixture: fixture, count: 2, theme: "sapphire-night", language: "zh-Hans")
        try await session.step("收起 2 人：宽 330，左 1 运行，右 1 待处理") {
            let island = try await collapse(session, width: 330, height: 34)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "compact-n2", text: "对照 compact-n2：黑色圆角胶囊，高约 34。左侧青色细环、数字 1、一个约 20pt 头像。右侧一个带橙色环的约 20pt 头像和橙色徽章 1。没有绿色对勾，没有摄像头圆点，没有主画布。")
            ]), "01-compact-n2.png")
            return "窗口 \(island.width)×\(island.height)"
        }
        try await phase(session, database: database, fixture: fixture, count: 5, theme: "sapphire-night", language: "zh-Hans")
        try await session.step("收起 5 人：宽 416，3 运行 + 2 待处理") {
            let island = try await collapse(session, width: 416, height: 34)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "compact-n5", text: "对照 compact-n5：胶囊加宽。左侧青色环、数字 3、三个互相重叠约 2pt 的 20pt 头像，没有 +N。右侧两个橙色环头像和橙色徽章 2。左右没有同一个人。")
            ]), "02-compact-n5.png")
            return "窗口 \(island.width)×\(island.height)"
        }
        try await phase(session, database: database, fixture: fixture, count: 8, theme: "sapphire-night", language: "zh-Hans")
        // 新的审批会自己弹出 540。完成通知 3 秒后收回成收起，不会再把已经看过的审批叫出来。
        // 所以先在收起之前截审批队列，再 Escape，再结束 Leo。
        try await session.step("审批提醒：540 宽，队列可切换") {
            let island = try await waitSize(session, width: 540, minHeight: 120, maxHeight: 640)
            try await session.selectNativeWindow(island.number)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "alert", text: "对照 alert：顶左橙色点、需要你、3 件待处理。卡片约 40pt 头像。下方待处理队列有三个约 22pt 头像，选中的一个是等待表情且不透明，未选中的是平静表情且不透明（不是 55% 排队）。右侧还有 N 个。")
            ]), "05-alert.png")
            try await session.visualClick("点击待处理队列里当前未选中的第二个头像，不要点允许、拒绝或查看成果")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "alert-switch", text: "队列选中项已切换：新的头像呈等待表情，原先选中的变为平静全不透明。卡片内容换成另一个人，待处理总数仍是 3。")
            ]), "06-alert-switch.png")
            return "队列点击已切换当前卡，窗口 \(island.width)×\(island.height)"
        }
        try await session.step("收起 8 人：左 3++1，右 2 与总数 3") {
            let island = try await collapse(session, width: 416, height: 34)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "compact-n8", text: "对照 compact-n8：左侧青色环、数字 4、三个重叠 20pt 头像和一个 +1 深灰胶囊。右侧两个橙色环头像和橙色徽章 3。没有摄像头圆点。")
            ]), "03-compact-n8.png")
            return "窗口 \(island.width)×\(island.height) 编号 \(island.number)"
        }
        try await session.step("完成通知：Leo 结束后 460 宽，倒计时在底边") {
            let full = try await waitSize(session, width: 416, minHeight: 33, maxHeight: 35)
            try await session.selectNativeWindow(full.number)
            try IslandRedesignFixture.finishLeo(fixture)
            var seen = false
            for _ in 0..<24 {
                if let island = try await session.nativeWindows().first(where: { abs($0.width - 460) <= 1 && $0.height > 40 }) {
                    try await session.selectNativeWindow(island.number)
                    seen = true
                    break
                }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            guard seen else { throw E2EError.product("完成通知没有把岛拉到 460pt：\(try await session.nativeWindows())") }
            try session.save(try await session.visualSnapshotCheck([
                VisualCriterion(id: "activity", text: "对照 activity：宽胶囊。顶左绿色对勾和已完成，顶右 norma-cloud。中间约 30pt 笑眼头像，文案含 Leo 完成了 push relay 失败重试，以及 3 个文件。白底查看成果按钮。底边一条绿色倒计时线。")
            ]), "04-activity.png")
            return "已截到 460 宽完成通知"
        }
        try await session.step("展开需要你处理，再看运行中筛选") {
            // 完成通知收回后是收起胶囊。页签只在展开后出现，先点胶囊。
            try await session.visualClick("点击顶部黑色收起胶囊的正中，把岛展开。不要点主窗口，不要点胶囊外的菜单栏")
            _ = try await waitSize(session, width: 660, minHeight: 120, maxHeight: 700)
            try await session.visualClick("点击运行中页签")
            let running = try await waitSize(session, width: 660, minHeight: 120, maxHeight: 700)
            guard abs(running.height - 52) > 2 else { throw E2EError.product("展开后只有头部，没有列表") }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "expanded-running", text: "对照 expanded-running：宽 660，顶栏约 52pt，标题运行概览。三个页签，运行中选中且徽章为 4。筛选条有全部、运行中、等你、完成和右侧一排约 20pt 头像。分组为 Norma-AI-OS、norma-cloud、multisoul。行内头像约 30pt。每个有记录的行是三层：第一行 名称 · provider · 会话名；第二行是人形小图标加用户最近一次输入；第三行是等宽字体的最近工具，如 Bash 加说明，运行中的行前是青色空心圈，其余是灰点，运行中行右侧有耗时。三行都单行截断不换行。排队的 Daisy 第二行是尚未开始，没有第三行，芯片是待开始，没有定位按钮。")
            ]), "07-expanded-running.png")
            try await session.visualClick("点击筛选条里的等你，不要点页签")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "filter-waiting", text: "等你筛选被选中。列表只留下等你处理的人，不包含运行中、已完成和 Daisy。筛选条上的数字与行数一致。")
            ]), "08-filter-waiting.png")
            let chip = try await session.visualLocate("定位筛选条上全部按钮的中心，不要点页签")
            let frame = try await session.capturePixels()
            try await session.hover(local: chip.point, on: frame, dwell: 200_000_000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "filter-hover", text: "鼠标悬停在全部上：该项比未悬停的筛选更深或有明显底色，等你仍是选中态，两者可以区分。")
            ]), "09-filter-hover.png")
            try await session.press(local: chip.point, on: frame) {
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "filter-pressed", text: "全部处于按下态，底色比悬停更深。松手前界面没有跳走。")
                ]), "10-filter-pressed.png")
            }
            try await session.visualClick("点击需要你处理页签")
            _ = try await waitSize(session, width: 660, minHeight: 120, maxHeight: 700)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "expanded-pending", text: "对照 expanded-pending：需要你处理页签选中，徽章 3。能看到权限、提问和待查看成果三类卡片，头像约 40pt。没有运行中的空态。")
            ]), "11-expanded-pending.png")
            try await session.visualClick("点击幕僚页签")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "expanded-staff", text: "对照 expanded-staff：幕僚页签选中。紫色幕僚标志和原有幕僚面板还在，没有被运行列表替换，也没有新的声波。")
            ]), "12-expanded-staff.png")
            return "展开宽 \(running.width)×\(running.height)；筛选悬停与按下已捕获"
        }
        try await session.step("英文与浅色下收起胶囊仍是 416×34") {
            session.stop()
            clearSignals(fixture)
            try IslandRedesignFixture.seed(database: database, count: 8)
            try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: "paper-blue")
            try session.launch(language: "en")
            try await openAll(session)
            try await waitReady(fixture, ids: [1, 2, 3, 4, 5, 6, 7])
            let island = try await collapse(session, width: 416, height: 34)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "compact-en-light", text: "浅色主题下岛本身仍是黑色胶囊。左侧数字 4 和 +1，右侧橙色徽章 3。没有中文需要你。展开前这一帧保持收起。")
            ]), "13-compact-en-light.png")
            try await session.visualClick("点击黑色胶囊展开岛")
            try await session.visualClick("点击 Running 页签")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "filters-en", text: "筛选条英文为 All、Running、Waiting、Done。排队行芯片是 Not started。等你处理芯片是 Needs you。页签含 Needs you、Running、Staff。")
            ]), "14-filters-en.png")
            return "浅色英文窗口 \(island.width)×\(island.height)"
        }
        try await session.step("减少动态效果：完成通知的倒计时不走动") {
            session.stop()
            clearSignals(fixture)
            try IslandRedesignFixture.seed(database: database, count: 4)
            try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: "sapphire-night")
            setenv("MULTISOUL_REDUCE_MOTION", "1", 1)
            try session.launch(language: "zh-Hans")
            try await openWorkspace(session, "norma-cloud", fromProject: false)
            try await waitReady(fixture, ids: [4])
            _ = try await collapse(session, width: 330, height: 34)
            let started = Date()
            try IslandRedesignFixture.finishLeo(fixture)
            var activity: NativeWindowInfo?
            for _ in 0..<16 {
                if let island = try await session.nativeWindows().first(where: { abs($0.width - 460) <= 1 }) {
                    try await session.selectNativeWindow(island.number)
                    activity = island
                    break
                }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            guard let activity else { throw E2EError.product("减少动态效果下没有 460 宽完成通知") }
            guard Date().timeIntervalSince(started) < 2.2 else {
                throw E2EError.product("完成通知出现得太晚，400ms 对照会跨过 3 秒收回")
            }
            let first = try await session.capturePixels()
            try await Task.sleep(nanoseconds: 400_000_000)
            let second = try await session.capturePixels()
            guard first.windowFrame.width == second.windowFrame.width else { throw E2EError.product("采样期间窗口宽度变化") }
            let same = first.pngData == second.pngData
            try session.save(first, "15-reduce-motion-a.png")
            try session.save(second, "16-reduce-motion-b.png")
            guard same else { throw E2EError.product("减少动态效果下 400ms 内两帧像素不同，倒计时或头像仍在动") }
            try session.save(try await session.visualSnapshotCheck([
                VisualCriterion(id: "reduce-motion", text: "完成通知底边的绿线铺满整条宽度，不是一根正在缩短的短线。头像是静止的笑眼，没有明显的弹跳中间态。")
            ]), "17-reduce-motion.png")
            return "减少动态效果两帧一致，窗口 \(activity.width)×\(activity.height)"
        }
    }

    private static func phase(_ session: Session, database: URL, fixture: URL, count: Int, theme: String, language: String) async throws {
        session.stop()
        clearSignals(fixture)
        try IslandRedesignFixture.seed(database: database, count: count)
        try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: theme)
        try session.launch(language: language)
        if count >= 1 { try await openWorkspace(session, "Norma-AI-OS", fromProject: false) }
        if count >= 4 { try await openWorkspace(session, "norma-cloud", fromProject: true) }
        // Mia（5）在 multisoul。5 人收起要有 3 个运行中，所以这一档就要打开，不能等到 6 人。
        if count >= 5 { try await openWorkspace(session, "multisoul", fromProject: true) }
        let ids = [1, 2, 3, 4, 5, 6, 7].filter { $0 <= count && $0 != 8 }
        try await waitReady(fixture, ids: ids.filter { $0 != 8 })
    }

    private static func openAll(_ session: Session) async throws {
        try await openWorkspace(session, "Norma-AI-OS", fromProject: false)
        try await openWorkspace(session, "norma-cloud", fromProject: true)
        try await openWorkspace(session, "multisoul", fromProject: true)
    }

    /// 首页直接点卡片标题。已经进入项目后，切换条的卡片经过相机缩放，
    /// 画面上的标题和命中框不重合，点标题或预览都进不去。
    /// 与工作空间场景相同：⇧⌘↑ 打开切换条，方向键移动选中描边，⌘↓ 打开。
    private static func openWorkspace(_ session: Session, _ name: String, fromProject: Bool) async throws {
        if fromProject {
            guard let main = try await session.nativeWindows().first(where: { $0.width > 900 }) else {
                throw E2EError.product("没有可切换的主窗口")
            }
            try await session.selectNativeWindow(main.number)
            try await session.shortcut(key: 126, modifiers: [0x37, 0x38])
            // 相机转场结束后再看选中描边，避免量到半程位置。
            try await Task.sleep(nanoseconds: 700_000_000)
            try await moveSwitcherSelection(session, to: name)
            try await session.shortcut(key: 125, modifiers: [0x37])
            return
        }
        // 首页卡片的标题在预览下方。只有一个项目时，点中卡片后会自己进入画布。
        try await session.visualClick("点击标题文字正好是「\(name)」的工作空间卡片。点必须落在这几个字上，不要点预览图、不要点省略号、不要点新建")
    }

    /// 打开切换条时选中项就是当前空间。三张卡片标题中心大约相距 390pt。
    /// 命令坞仍占焦点时，方向键不会移动卡片；空搜索框获得焦点后才会转发。
    private static func moveSwitcherSelection(_ session: Session, to name: String) async throws {
        try await session.visualClick("点击顶部切换条里「搜索工作空间」输入框的正中。不要点新建工作空间，不要点任何卡片")
        let target = try await session.visualLocate("定位标题文字正好是「\(name)」的切换条卡片。点落在这几个字的中心，不要点预览图、省略号或新建")
        let selected = try await session.visualLocate("定位当前带整圈选中描边的切换条卡片，点在它的标题文字中心。不要点预览图内部，不要点没有描边的卡片")
        let steps = Int(((target.point.x - selected.point.x) / 390).rounded())
        guard abs(steps) <= 6 else {
            throw E2EError.product("切换条从选中项到「\(name)」的步数异常：\(steps)，\(selected.point) -> \(target.point)")
        }
        let key: UInt16 = steps >= 0 ? 124 : 123
        for _ in 0..<abs(steps) {
            try await session.shortcut(key: key, modifiers: [])
            try await Task.sleep(nanoseconds: 150_000_000)
        }
        let now = try await session.visualLocate("定位当前带整圈选中描边的切换条卡片，点在它的标题文字中心")
        guard abs(now.point.x - target.point.x) < 80 else {
            throw E2EError.product("方向键没有把选中项移到「\(name)」：现在 \(now.point)，目标 \(target.point)，步数 \(steps)")
        }
    }

    /// 审批到达前岛会先停在收起尺寸。必须等它展开过再按 Escape，并稳住超过看图前的 600ms，
    /// 否则截图会落在随后弹出的审批上。没有审批时，连续 3 秒保持目标尺寸也算收起。
    private static func collapse(_ session: Session, width: Double, height: Double) async throws -> NativeWindowInfo {
        var sawLarger = false
        var stable = 0
        for _ in 0..<40 {
            if let island = try await session.nativeWindows().first(where: { $0.width >= 300 && $0.width < 800 }) {
                try await session.selectNativeWindow(island.number)
                let frame = try await session.capturePixels()
                guard frame.windowFrame.width < 800 else {
                    throw E2EError.driver("选中岛之后截到的不是岛：\(frame.windowFrame)")
                }
                let matches = abs(island.width - width) <= 1 && abs(island.height - height) <= 1
                if matches {
                    stable += 1
                    let needed = sawLarger ? 5 : 15
                    if stable >= needed { return island }
                } else {
                    sawLarger = true
                    stable = 0
                    try await session.shortcut(key: 53, modifiers: [])
                }
            } else {
                stable = 0
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        return try await waitSize(session, width: width, minHeight: height - 1, maxHeight: height + 1)
    }

    private static func waitSize(_ session: Session, width: Double, minHeight: Double, maxHeight: Double) async throws -> NativeWindowInfo {
        for _ in 0..<25 {
            if let island = try await session.nativeWindows().first(where: {
                abs($0.width - width) <= 1 && $0.height >= minHeight && $0.height <= maxHeight
            }) {
                return island
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("岛没有变成 \(width)×\(minHeight)...\(maxHeight)：\(try await session.nativeWindows())")
    }

    private static func waitReady(_ root: URL, ids: [Int]) async throws {
        let deadline = Date().addingTimeInterval(40)
        while Date() < deadline {
            let ready = ids.allSatisfy { FileManager.default.fileExists(atPath: root.appendingPathComponent("ready-\($0)").path) }
            if ready { return }
            let failed = ids.filter { FileManager.default.fileExists(atPath: root.appendingPathComponent("failed-\($0)").path) }
            if !failed.isEmpty {
                let log = (try? String(contentsOf: root.appendingPathComponent("agent.log"), encoding: .utf8)) ?? ""
                throw E2EError.product("角色 \(failed) Hook 失败。\(log.suffix(800))")
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        let log = (try? String(contentsOf: root.appendingPathComponent("agent.log"), encoding: .utf8)) ?? ""
        throw E2EError.product("角色 \(ids) 没有在 40 秒内就绪。\(log.suffix(800))")
    }

    private static func clearSignals(_ root: URL) {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: root.path) else { return }
        for name in names where name.hasPrefix("ready-") || name.hasPrefix("failed-") || name == "finish-leo" || name == "agent.log" {
            try? FileManager.default.removeItem(at: root.appendingPathComponent(name))
        }
    }
}

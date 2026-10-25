import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 幕僚活动面板：预置 7 天内外的记录，验收打开、筛选、展开正文、“来自第 N 轮”跳转、
/// 未读数清零，以及退出重开后 7 天内的记录仍在、过期的已被清理。
@MainActor
public enum BrainActivity {
    public nonisolated static let name = "brain-activity"
    typealias Text = BrainActivityFixture.Text

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
        var database = URL(fileURLWithPath: "/")

        try await session.step("首次启动建库，停止后写入 7 天内外的幕僚记录") {
            try session.launch()
            // 只需要库建好表，不必识别画面：少做文字识别，避免 Vision 的精确模型在长进程里失效。
            database = try session.databaseURL()
            let deadline = Date().addingTimeInterval(60)
            while (try? SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM sqlite_master WHERE name='brain_turns'")) != 1 {
                guard Date() < deadline else { throw E2EError.product("首次启动后没有建出 brain_turns 表") }
                try await Task.sleep(nanoseconds: 300_000_000)
            }
            session.stop()
            try BrainActivityFixture.seed(database: database)
            let counts = try BrainActivityFixture.counts(database: database)
            guard counts.turns == 5, counts.actions == 4 else {
                throw E2EError.product("预置记录数量不对：\(counts)")
            }
            return "写入 5 轮、4 个动作（其中 8 天前 1 轮、1 个动作）"
        }

        try await session.step("重新启动：写入器清掉 8 天前的记录，3 个动作都还没读过") {
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 90))
            let counts = try await waitCounts(database) { $0.turns == 4 && $0.actions == 3 }
            guard counts.unseen == 3 else { throw E2EError.product("打开面板前应有 3 个未读动作：\(counts)") }
            return "清理后 \(counts)"
        }

        try await session.step("⌘⇧B 打开面板：按时间列出轮次与动作，过期记录不在，未读数清零") {
            let frame = try await openPanel(session)
            try session.save(frame, "01-panel.png")
            for needle in ["保留最近", "补上", Text.finishedSummary, Text.needsSummary, "当前项目"] {
                guard frame.contains(needle) else { throw E2EError.product("面板里缺少「\(needle)」：\(frame.summary)") }
            }
            for needle in ["八天前", "过期的派发"] {
                guard !frame.contains(needle) else { throw E2EError.product("过期记录不该出现「\(needle)」：\(frame.summary)") }
            }
            let counts = try await waitCounts(database) { $0.unseen == 0 }
            return "未读 \(counts.unseen)"
        }

        try await session.step("筛选：自主动作只剩动作，需要你只剩没有结果的审批轮次，全部恢复") {
            try await clickInLine(session, "自主动作", pick: .top)
            let actions = try await session.wait(Expectation(allOf: ["补上"], absent: [Text.finishedSummary, Text.needsSummary], timeout: 6))
            try session.save(actions, "02-filter-actions.png")
            try await clickNeedsPill(session)
            let needs = try await session.wait(Expectation(allOf: [Text.needsSummary], absent: ["补上", Text.finishedSummary], timeout: 6))
            try session.save(needs, "03-filter-needs.png")
            try await clickInLine(session, "全部", pick: .left)
            _ = try await session.wait(Expectation(allOf: ["补上", Text.finishedSummary, Text.needsSummary], timeout: 6))
            return "三种筛选都生效"
        }

        try await session.step("点一轮展开正文，再点收起") {
            try await session.click(matching: [Text.finishedSummary], pick: .top)
            let open = try await session.wait(Expectation(allOf: ["复制本轮", Text.promptMarker], timeout: 8))
            try session.save(open, "04-turn-expanded.png")
            try await session.click(matching: [Text.promptMarker], pick: .top)
            _ = try await session.wait(Expectation(absent: ["复制本轮"], timeout: 6))
            return "展开显示输入正文和复制按钮，收起后消失"
        }

        try await session.step("动作行点“来自第 1 轮”：清筛选、展开并回到那一轮") {
            try await clickInLine(session, "自主动作", pick: .top)
            _ = try await session.wait(Expectation(allOf: ["补上"], absent: [Text.finishedSummary], timeout: 6))
            try await session.click(matching: ["补上"], pick: .top)
            let expanded = try await session.wait(Expectation(allOf: ["来自第", "理由"], timeout: 8))
            try session.save(expanded, "05-action-expanded.png")
            try await session.click(matching: ["来自第"], pick: .top)
            let jumped = try await session.wait(Expectation(allOf: [Text.finishedSummary, "复制本轮"], timeout: 8))
            try session.save(jumped, "06-jumped.png")
            guard jumped.contains(Text.needsSummary) else {
                throw E2EError.product("跳转后筛选没有恢复为全部：\(jumped.summary)")
            }
            return "跳到第 1 轮并展开"
        }

        try await session.step("Esc 关闭面板；退出重开后 7 天内的记录仍在，过期的没有，未读仍为 0") {
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.wait(Expectation(absent: ["保留最近"], timeout: 6))
            session.stop()
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 90))
            let frame = try await openPanel(session)
            try session.save(frame, "07-after-restart.png")
            for needle in ["补上", Text.finishedSummary, "五天前"] {
                guard frame.contains(needle) else { throw E2EError.product("重启后缺少「\(needle)」：\(frame.summary)") }
            }
            guard !frame.contains("八天前") else { throw E2EError.product("重启后出现过期记录：\(frame.summary)") }
            let counts = try BrainActivityFixture.counts(database: database)
            guard counts.turns == 4, counts.actions == 3, counts.unseen == 0 else {
                throw E2EError.product("重启后数据库状态不对：\(counts)")
            }
            return "重启后 \(counts)"
        }
    }

    /// ⌘⇧B 偶尔被刚结束的输入吞掉：没看到面板就再按，最多三次。
    static func openPanel(_ session: Session) async throws -> Frame {
        for _ in 0..<3 {
            try await session.shortcut(key: 11, modifiers: [0x37, 0x38])
            let shown = (try? await session.wait(Expectation(allOf: ["保留最近"], timeout: 6))) != nil
            if shown {
                try await Task.sleep(nanoseconds: 800_000_000)
                return try await session.capture()
            }
        }
        throw E2EError.product("⌘⇧B 三次都没有打开幕僚活动面板：\(try await session.capture().summary)")
    }

    static func waitCounts(
        _ database: URL, timeout: TimeInterval = 12,
        _ ready: ((turns: Int, actions: Int, unseen: Int)) -> Bool
    ) async throws -> (turns: Int, actions: Int, unseen: Int) {
        let deadline = Date().addingTimeInterval(timeout)
        var last = (turns: -1, actions: -1, unseen: -1)
        while Date() < deadline {
            if let counts = try? BrainActivityFixture.counts(database: database) {
                last = counts
                if ready(counts) { return counts }
            }
            try await Task.sleep(nanoseconds: 300_000_000)
        }
        throw E2EError.product("等待幕僚活动记录超时。最近：\(last)")
    }

    /// 文字识别常把同一行的几个胶囊合成一条：按目标词在整行里的位置估算横坐标再点击。
    static func clickInLine(_ session: Session, _ word: String, pick: TextPick) async throws {
        let frame = try await session.capture()
        guard let line = frame.pick([word], pick), let range = line.value.range(of: word) else {
            throw E2EError.product("找不到「\(word)」：\(frame.summary)")
        }
        let total = max(line.value.count, 1)
        let middle = line.value.distance(from: line.value.startIndex, to: range.lowerBound) + word.count / 2
        let x = line.rect.minX + line.rect.width * (CGFloat(middle) + 0.5) / CGFloat(total)
        try await session.click(local: CGPoint(x: x, y: line.rect.midY), on: frame)
        try await Task.sleep(nanoseconds: 500_000_000)
    }
}

extension BrainActivity {
    /// “需要你 1”胶囊偶尔识别不出来：退回到紧挨在“对话轮次”胶囊右边的位置点击（相隔约一个胶囊的半宽）。
    static func clickNeedsPill(_ session: Session) async throws {
        let frame = try await session.capture()
        if let pill = frame.pick(["需要你1", "需要你 1"], .top), pill.rect.midY < 140 {
            try await session.click(local: pill.center, on: frame)
        } else if let anchor = frame.pick(["对话轮次"], .top) {
            try await session.click(local: CGPoint(x: anchor.rect.maxX + 58, y: anchor.rect.midY), on: frame)
        } else {
            throw E2EError.product("找不到筛选胶囊：\(frame.summary)")
        }
        try await Task.sleep(nanoseconds: 500_000_000)
    }
}

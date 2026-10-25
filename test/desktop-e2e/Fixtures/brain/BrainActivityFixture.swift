import E2EKit
import Foundation

/// 幕僚活动面板验收用的记录：7 天内 4 轮、3 个动作，另有 8 天前 1 轮、1 个动作（启动时应被清理）。
/// 区域 id 用空串，面板里显示为“当前项目”。
public enum BrainActivityFixture {
    public static let launch = "7"
    public static let hour = 3_600_000
    public static let day = 24 * hour

    /// 面板里应能读到的文字（避开 OCR 容易认错的生僻字）。
    public enum Text {
        public static let finishedSummary = "没有下一步"
        public static let assignedSummary = "补上登录页的测试"
        public static let needsSummary = "拿不准"
        public static let promptMarker = "FIXTURE"
        public static let expiredSummary = "八天前的过期记录"
        public static let expiredAction = "过期的派发"
    }

    public static func seed(database: URL, now: Date = Date()) throws {
        let t = Int(now.timeIntervalSince1970 * 1000)
        let hour = Self.hour, day = Self.day
        var sql: [(String, [String])] = []
        func turn(_ n: Int, _ lane: String, _ at: Int, _ trigger: String, _ who: String, _ stop: Bool = true) {
            sql.append((
                "INSERT INTO brain_turns(launch_id,area_id,lane,turn,started_at_ms,trigger_kind,trigger_who,finished_at_ms,stop_reason) VALUES(\(launch),'',?1,\(n),\(at),?2,?3,NULLIF(?4,''),NULLIF(?5,''))",
                [lane, trigger, who, stop ? String(at + 9_000) : "", stop ? "end_turn" : ""]
            ))
        }
        func item(_ n: Int, _ lane: String, _ at: Int, _ kind: String, _ text: String) {
            sql.append((
                "INSERT INTO brain_trace_items(launch_id,area_id,lane,turn,at_ms,kind,title,text) VALUES(\(launch),'',?1,\(n),\(at),?2,'',?3)",
                [lane, kind, text]
            ))
        }
        func action(_ at: Int, _ kind: String, _ agent: String, _ summary: String, _ reason: String, via: (String, Int)?) {
            sql.append((
                "INSERT INTO brain_actions(launch_id,area_id,at_ms,kind,agent,node_id,summary,reason,ask_key,via_lane,via_turn) VALUES(\(launch),'',\(at),?1,?2,3,?3,?4,'assign',NULLIF(?5,''),CAST(NULLIF(?6,'') AS INTEGER))",
                [kind, agent, summary, reason, via?.0 ?? "", via.map { String($0.1) } ?? ""]
            ))
        }
        // 第 1 轮：Alan 完成后大脑派发后续任务
        turn(1, "Main", t - 3 * hour, "agent_finished", "Alan")
        item(1, "Main", t - 3 * hour + 1, "prompt", "\(Text.promptMarker) Alan 完成了登录页")
        item(1, "Main", t - 3 * hour + 8_000, "reply", "\(Text.finishedSummary)，登录页的测试已经补齐。")
        action(t - 3 * hour + 5_000, "assigned", "Chloe", Text.assignedSummary, "Chloe 空闲，按目标派后续任务", via: ("Main", 1))
        // 第 2 轮：审批会话放行
        turn(2, "Approval", t - 2 * hour, "approval_request", "Claude")
        item(2, "Approval", t - 2 * hour + 1, "prompt", "审批 cargo test")
        item(2, "Approval", t - 2 * hour + 5_000, "reply", "低风险，已放行。")
        action(t - 2 * hour + 4_000, "approved", "Claude", "cargo test -p brain", "只读测试命令", via: ("Approval", 2))
        // 用户在概览里批准的派发：没有来源轮次
        action(t - hour, "user", "Bella", "整理发布说明", "你批准了幕僚的提议", via: nil)
        // 第 3 轮：审批会话拿不准，没有产生动作
        turn(3, "Approval", t - 30 * 60_000, "approval_request", "Alan")
        item(3, "Approval", t - 30 * 60_000 + 1, "prompt", "审批 git push")
        item(3, "Approval", t - 30 * 60_000 + 6_000, "reply", "\(Text.needsSummary)，这类操作交给你决定。")
        // 第 4 轮：5 天前，仍在 7 天内
        turn(4, "Main", t - 5 * day, "scheduled", "")
        item(4, "Main", t - 5 * day + 1, "reply", "五天前的定时摘要，没有新进展。")
        // 8 天前：应被启动时的清理删掉
        turn(5, "Main", t - 8 * day, "scheduled", "")
        item(5, "Main", t - 8 * day + 1, "reply", Text.expiredSummary)
        action(t - 8 * day + 2, "assigned", "Alan", Text.expiredAction, "过期", via: ("Main", 5))
        try SQLRaw.execute(database: database, statements: sql)
    }

    public static func counts(database: URL) throws -> (turns: Int, actions: Int, unseen: Int) {
        let turns = try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM brain_turns") ?? -1
        let actions = try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM brain_actions") ?? -1
        let unseen = try SQLRaw.scalar(
            database: database,
            "SELECT COUNT(*) FROM brain_actions WHERE at_ms > COALESCE((SELECT value FROM brain_activity_state WHERE key='actions_seen_at_ms'),0)"
        ) ?? -1
        return (turns, actions, unseen)
    }
}

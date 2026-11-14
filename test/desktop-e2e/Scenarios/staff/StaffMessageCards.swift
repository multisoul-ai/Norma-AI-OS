import CoreGraphics
import E2EKit
import Fixtures
import Foundation

@MainActor
enum StaffMessageCards {
    static func answer(_ session: Session, root: URL) async throws {
        try await waitFor("两张并发提问卡") { try questions(root).filter { $0["state"] as? String == "Pending" }.count == 2 }
        try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: -3000)
        let pending = try await session.capturePixels()
        // 翻译会改变操作按钮宽度，按当前画面定位，不能复用中文 Skip 的横坐标。
        try await session.visualClick("点击 Enable optional analytics? 问题卡的跳过/Skip按钮")
        try await waitFor("第二张问题已跳过") { try questions(root).contains { $0["state"] as? String == "Skipped" } }
        try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: 3000)
        for point in [CGPoint(x: 365, y: 457), CGPoint(x: 365, y: 624), CGPoint(x: 365, y: 680)] {
            try await session.click(local: point, on: try await session.capturePixels())
        }
        let choices = try await session.capturePixels()
        try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: -300)
        try await session.visualClick("点击 Private build label 题的密码输入框")
        try await session.type("e2e-private-label")
        let form = try await session.capturePixels()
        try await session.visualClick("点击第一张包含主题和检查项问答卡的提交回答/Submit answers按钮")
        try await waitFor("已回答和跳过状态") {
            let rows = try questions(root)
            return rows.contains { $0["state"] as? String == "Answered" } && rows.contains { $0["state"] as? String == "Skipped" }
        }
        try session.save(try await session.visualSnapshotCheck(pending, [VisualCriterion(id: "pending", text: "中央有待回答的问题卡，末张题目Enable optional analytics?有Yes/No和Skip/跳过；左侧任务显示需要你/Needs you及2题待答计数。")]), "cards-00-pending.png")
        _ = try await session.visualSnapshotCheck(choices, [VisualCriterion(id: "choices", text: "主题Dark单选选中，Keyboard shortcuts与Accessibility两个方框同时打勾；控件与题目不重叠。")])
        try session.save(try await session.visualSnapshotCheck(form, [VisualCriterion(id: "form", text: "Private build label输入为掩码而非明文，提交回答/Submit answers可用；多选的Keyboard shortcuts和Accessibility保持选中，控件不重叠。")]), "cards-01-answers.png")
        let snapshot = try JSONSerialization.data(withJSONObject: StaffFixture.tasks(root))
        guard !String(decoding: snapshot, as: UTF8.self).contains("e2e-private-label") else { throw E2EError.product("密码泄漏到任务对话快照") }
        guard try StaffFixture.records(root,"question-replies.jsonl").count == 1 else { throw E2EError.product("并发原工具没有恰好释放一次") }
    }

    static func ignoreAndRestore(_ session: Session, root: URL) async throws {
        try await session.visualClick("点击提议卡的忽略/Ignore按钮")
        try await waitFor("忽略落盘") { try proposal(root)["ignored"] as? Bool == true }
        _ = try await session.visualCheck([VisualCriterion(id: "ignored", text: "原提议折叠为虚线边框的已忽略/Ignored单行，含标题；采用和编辑按钮消失。")])
        try await session.visualClick("点击已忽略/Ignored提议的单行，展开")
        _ = try await session.visualCheck([VisualCriterion(id: "restore", text: "已忽略提议展开后显示原任务内容和恢复提议/Restore proposal按钮，未自动派发。")])
        try await session.visualClick("点击恢复提议/Restore proposal按钮")
        try await waitFor("恢复落盘") { try proposal(root)["ignored"] as? Bool == false }
        try session.save(try await session.visualCheck([VisualCriterion(id: "restored", text: "提议恢复完整卡片，采用并派发/Adopt and dispatch按钮重新可用。")]), "cards-02-restored.png")
        try await session.scroll(xFraction: 0.48,yFraction: 0.55,deltaY: -3000)
        _ = try await session.visualCheck([VisualCriterion(id: "activity", text: "工具活动摘要显示已处理/Processed、步数和用时，没有仍在转的等待动画；report越权调用以橙色的已拒绝/Denied状态区分。")])
        try await session.visualClick("点击已拒绝/Denied的report工具调用行，展开输出")
        try await session.scroll(xFraction: 0.48,yFraction: 0.55,deltaY: -3000)
        try session.save(try await session.visualCheck([VisualCriterion(id: "output", text: "拒绝的工具行展开后可读调用入参及拒绝输出；长入参被截断，布局不横向溢出。")]), "cards-03-tools.png")
        try await session.visualClick("点击展开的report工具行，收起详细入参与输出")
        // 收起工具详情后回到底部，让后续提议操作按钮保持可见。
        try await session.scroll(xFraction: 0.48,yFraction: 0.55,deltaY: -3000)
    }

    static func answerWorker(_ session: Session, root: URL) async throws {
        try await waitFor("Worker原生提问镜像") { try questions(root).contains { $0["worker"] is [String: Any] && $0["state"] as? String == "Pending" } }
        try await session.scroll(xFraction: 0.48,yFraction: 0.55,deltaY: -3000)
        try await session.scroll(xFraction: 0.48,yFraction: 0.55,deltaY: 220)
        _ = try await session.visualCheck([VisualCriterion(id: "worker-question", text: "中央对话出现Worker的问题/Worker question卡，题目Record acceptance notes?，有Yes/No选项和提交按钮。")])
        try await session.visualClick("点击Record acceptance notes?题中的Yes选项")
        try await session.visualClick("点击Worker问题卡的提交回答/Submit answers按钮")
        try await waitFor("原Hook恰好收到一条回答") { try StaffFixture.records(root,"worker-question-replies.jsonl").count == 1 }
        try await waitFor("镜像已回答终态") { try questions(root).contains { $0["worker"] is [String: Any] && $0["state"] as? String == "Answered" } }
    }

    private static func questions(_ root: URL) throws -> [[String: Any]] {
        let tasks = try StaffFixture.tasks(root)
        return tasks.flatMap { ($0["messages"] as? [[String: Any]] ?? []).compactMap { ($0["Question"] as? [String: Any])?["question"] as? [String: Any] } }
    }
    private static func proposal(_ root: URL) throws -> [String: Any] { (try StaffFixture.tasks(root).first?["proposals"] as? [[String: Any]])?.first ?? [:] }
    private static func waitFor(_ description: String, _ check: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(60)
        while Date() < deadline {
            if try check() { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("等待超时：\(description)")
    }
}

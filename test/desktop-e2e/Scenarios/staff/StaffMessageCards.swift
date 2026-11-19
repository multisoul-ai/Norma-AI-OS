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
        for point in [CGPoint(x: 365, y: 457), CGPoint(x: 365, y: 624)] {
            try await session.click(local: point, on: try await session.capturePixels())
        }
        let choices = try await session.capturePixels()
        try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: -300)
        // 顶部视图里 Accessibility 行落在输入框叠加层下面，下移 300px 后它与密码框都在可见区域。
        try await session.click(local: CGPoint(x: 365, y: 381), on: try await session.capturePixels())
        // 题卡许可只有 120 秒；密码框是整行等宽输入框，位置由固定字段顺序决定，这里用原始点击省掉一次模型往返。
        // 跳过按钮的横坐标随语言变化，仍按当前画面定位。
        try await session.click(local: CGPoint(x: 595, y: 520), on: try await session.capturePixels())
        try await session.type("e2e-private-label")
        let form = try await session.capturePixels()
        // 提交按钮是该行第一个按钮，左对齐、位置由字段高度决定；用原始点击把作答压在题卡 120 秒许可内。
        try await session.click(local: CGPoint(x: 388, y: 572), on: try await session.capturePixels())
        try await waitFor("已回答和跳过状态") {
            let rows = try questions(root)
            return rows.contains { $0["state"] as? String == "Answered" } && rows.contains { $0["state"] as? String == "Skipped" }
        }
        try session.save(try await session.visualSnapshotCheck(pending, [VisualCriterion(id: "pending", text: "中央有待回答的问题卡，末张题目Enable optional analytics?有Yes/No和Skip/跳过；左侧任务显示需要你/Needs you及2题待答计数。")]), "cards-00-pending.png")
        _ = try await session.visualSnapshotCheck(choices, [VisualCriterion(id: "choices", text: "顶部视图下主题Dark单选选中、Keyboard shortcuts已打勾；Accessibility与两个自定义回答未选中，各控件与题目文字不重叠、不被裁切。")])
        try session.save(try await session.visualSnapshotCheck(form, [VisualCriterion(id: "form", text: "下移后的完整表单里主题Dark选中，Keyboard shortcuts与Accessibility两个方框同时打勾；Private build label 显示为掩码圆点而非明文，提交回答/Submit answers 可用；控件与题目不重叠。")]), "cards-01-answers.png")
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
        try await session.visualClick("点击回复上方「已处理 N 步」过程区标题，展开步骤")
        _ = try await session.visualCheck([VisualCriterion(id: "activity", text: "过程区标题显示已处理/Processed、步数和用时，没有仍在转的等待动画；展开后 report 越权调用的步骤以橙色的已拒绝/Denied状态区分，步骤标题没有路径。")])
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
        // 原 Hook 只等 140 秒：先把这一帧留作稍后看图，勾选与提交用原始点击立刻完成，不让模型往返吃掉等待窗口。
        let questionFrame = try await session.capturePixels()
        try await session.click(local: CGPoint(x: 600, y: 405), on: try await session.capturePixels())
        try await session.click(local: CGPoint(x: 390, y: 587), on: try await session.capturePixels())
        try await waitFor("原Hook恰好收到一条回答") { try StaffFixture.records(root,"worker-question-replies.jsonl").count == 1 }
        try await waitFor("镜像已回答终态") { try questions(root).contains { $0["worker"] is [String: Any] && $0["state"] as? String == "Answered" } }
        _ = try await session.visualSnapshotCheck(questionFrame, [
            VisualCriterion(id: "worker-question", text: "中央对话出现Worker的问题/Worker question卡，题目Record acceptance notes?，有Yes（Include notes in the result）/No（Only the artifact）选项和提交回答/Submit answers按钮。"),
            VisualCriterion(id: "worker-needs-you-pet", text: "标题下 Worker 卡条中的 Worker 胶囊处于需要你/Needs you 状态（橙色圆点与橙色状态文字），胶囊最左侧是带眼睛与表情的彩色宠物头像，不是字母缩写圆或纯色圆点；胶囊内同时有真实 Agent 名称与引擎名。")
        ])
    }

    private static func questions(_ root: URL) throws -> [[String: Any]] {
        let tasks = try StaffFixture.tasks(root)
        return tasks.flatMap { $0["questions"] as? [[String: Any]] ?? [] }
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

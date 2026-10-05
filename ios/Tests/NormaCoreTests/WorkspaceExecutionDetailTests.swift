import Foundation
import Testing
@testable import NormaCore

private func executionEvent(_ cursor: Int, operation: String = "operation-a", run: String? = nil, session: String? = "session-a", kind: String = "submitted") throws -> WorkspaceChatEvent {
    let data = try JSONSerialization.data(withJSONObject: ["mac_id":"mac-a","desktop_workspace_id":"space-a",
        "device_id":"phone-a","operation_id":operation,"cursor":cursor,"kind":kind,
        "payload":["task_ref":"task-a","text":kind,"target":["area_id":"area-a","definition_id":1,"node_id":2,
            "run_id":run as Any? ?? NSNull(),"session_id":run == nil ? NSNull() : session as Any? ?? NSNull()]]])
    return try JSONDecoder().decode(WorkspaceChatEvent.self, from: data)
}

/// 场景：已知run但CLI会话ID迟到时，首个同run会话补全身份，此后不得替换。
/// 数据构造：cursor1为run-a且无session；cursor2补session-a；cursor3改为session-b。
/// 执行过程：解析running与两条结果 → 补全首会话 → 排除替代会话。
/// 预期结果：session-a正文可读；session-b不能取代原成果。
@Test func executionDetailFillsLateSessionWithinPinnedRun() throws {
    let initial = try executionEvent(1, run: "run-a", session: nil, kind: "running")
    let completed = try executionEvent(2, run: "run-a", kind: "task_result")
    let replacement = try executionEvent(3, run: "run-a", session: "session-b", kind: "task_result")
    let detail = workspaceExecutionDetail(initial, events: [initial, completed, replacement])
    #expect(detail.cursor == 2, "迟到的首会话应补全原执行结果")
    #expect(detail.payload.target?.sessionID == "session-a", "必须锁定首个session-a")
    #expect(detail.payload.target?.sessionID != "session-b", "不能自动替换成session-b")
}

/// 场景：打开执行回执后由同任务事件补全运行和成果，不借用后续任务的会话。
/// 数据构造：首回执无run；同operation完成于run-a；另一个operation为run-b。
/// 执行过程：选择首回执 → 同任务完成事件到达 → 后续任务事件到达 → 再查看旧详情。
/// 预期结果：原详情变成完成正文，绑定run-a；run-b不能替换原终端。
@Test func executionDetailBindsOnlyOriginalOperation() throws {
    let initial = try executionEvent(1)
    let completed = try executionEvent(2, run: "run-a", kind: "task_result")
    let other = try executionEvent(3, operation: "operation-b", run: "run-b", kind: "task_result")
    let detail = workspaceExecutionDetail(initial, events: [initial, completed, other])
    #expect(detail.payload.target?.runID == "run-a", "必须补全原任务的真实run")
    #expect(detail.kind == "task_result", "结果栏必须随同任务完成自动更新")
    #expect(detail.payload.target?.runID != "run-b", "后续任务不得替换旧成果的终端")
}

/// 场景：未绑定的回执一旦补全原会话，就不能被同任务的新会话替换。
/// 数据构造：cursor1无run；cursor2为run-a；cursor3为run-b，operation仍相同。
/// 执行过程：从初始回执解析所有后续事件 → 检查绑定身份。
/// 预期结果：锁定首次run-a；拒绝run-b，等待原会话的结果。
@Test func executionDetailPinsFirstBoundSession() throws {
    let initial = try executionEvent(1)
    let bound = try executionEvent(2, run: "run-a", kind: "running")
    let replacement = try executionEvent(3, run: "run-b", kind: "task_result")
    let detail = workspaceExecutionDetail(initial, events: [initial, bound, replacement])
    #expect(detail.payload.target?.runID == "run-a", "首次绑定之后不得自动替换会话")
    #expect(detail.kind != "task_result", "替代会话的结果不属于原执行")
}

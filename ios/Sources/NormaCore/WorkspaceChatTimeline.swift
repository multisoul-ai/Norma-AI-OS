import Foundation

/// Server cursor is the ordering authority. Operation IDs link replies without moving them in time.
public func workspaceChatTimeline(_ events: [WorkspaceChatEvent]) -> [WorkspaceChatEvent] {
    struct ResultKey: Hashable {
        let macID: String, workspaceID: String, deviceID: String, operationID: String
        let taskRef: String?, text: String?
    }
    func key(_ event: WorkspaceChatEvent) -> ResultKey {
        ResultKey(macID: event.macID, workspaceID: event.workspaceID, deviceID: event.deviceID,
                  operationID: event.operationID, taskRef: event.payload.taskRef, text: event.payload.text)
    }
    let results = Set(events.filter { $0.kind == "task_result" }.map(key))
    return events.filter { event in
        event.kind != "succeeded" || !results.contains(key(event))
    }.sorted { $0.cursor < $1.cursor }
}

public func workspaceChatStatus(_ value: String) -> String {
    switch value {
    case "pending": "待发送"
    case "received": "Mac 已接收"
    case "processing", "submitted", "queued", "running", "thinking", "running_tool": "处理中"
    case "succeeded", "done", "replied": "已完成"
    case "idle", "ready": "可用"
    case "preparing": "正在准备"
    case "interrupting": "正在中断"
    case "ended": "会话已结束"
    case "failed": "未能完成"
    case "outcome_unknown": "结果待核查，未自动重发"
    case "interrupted": "执行已中断"
    case "cancelled": "已取消"
    case "waiting_approval": "等待确认"
    case "waiting_input": "等待补充"
    case "untracked": "等待运行回执"
    default: "状态待核查"
    }
}

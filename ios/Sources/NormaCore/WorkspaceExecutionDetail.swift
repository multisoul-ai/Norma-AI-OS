import Foundation

/// Resolve only facts from the selected execution, never the current catalog's replacement run.
public func workspaceExecutionDetail(_ initial: WorkspaceChatEvent, events: [WorkspaceChatEvent]) -> WorkspaceChatEvent {
    guard let original = initial.payload.target else { return initial }
    let task = initial.payload.taskRef ?? original.taskRef
    let candidates = events.filter { event in
        guard event.cursor >= initial.cursor, event.macID == initial.macID, event.workspaceID == initial.workspaceID,
              event.deviceID == initial.deviceID, event.operationID == initial.operationID,
              let target = event.payload.target, target.areaID == original.areaID, target.nodeID == original.nodeID,
              target.definitionID == original.definitionID,
              (event.payload.taskRef ?? target.taskRef) == task,
              original.runID == nil || target.runID == original.runID,
              original.sessionID == nil || target.sessionID == original.sessionID else { return false }
        return ["running", "submitted", "task_result"].contains(event.kind)
    }.sorted { $0.cursor < $1.cursor }
    let run = original.runID ?? candidates.first(where: { $0.payload.target?.runID != nil })?.payload.target?.runID
    let session = original.sessionID ?? candidates.first(where: { $0.payload.target?.runID == run && $0.payload.target?.sessionID != nil })?.payload.target?.sessionID
    let pinned = candidates.filter { event in
        guard let run else { return true }
        return event.payload.target?.runID == run && event.payload.target?.sessionID == session
    }
    return pinned.last(where: { $0.kind == "task_result" }) ?? pinned.last ?? initial
}

import Foundation
import SQLite3

extension CollaborationWorkspaceStore {
    /// Freeze only current-thread history and workspace facts; nothing is dispatched while reasoning.
    public func prepareNormaTurn(threadID: String, messageID: String, text: String,
                                 selected: [WorkspaceAgentReference] = [], clearingDraftRevision: String? = nil) throws -> PreparedThreadPlan {
        try threadDatabase { db in
            let thread = try requireThread(db, threadID)
            try requireWorkspace(db, thread.workspaceID, writing: true)
            guard let row = try threadRows(db, "SELECT id,id,configuration FROM collaboration_workspaces WHERE id=?", [thread.workspaceID]).first else {
                throw WorkThreadError.notFound
            }
            let workspace = try decode(CollaborationWorkspace.self, row)
            try validate(workspace)
            let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard ThreadMentions.requestsNorma(text) else { throw WorkThreadError.invalidContent }
            let remaining = text.replacingOccurrences(of: ThreadMentions.normaPattern, with: "", options: [.regularExpression, .caseInsensitive])
            let addressed = try ThreadMentions.resolve(remaining, workspace: workspace, selected: selected)
            let source = WorkThreadMessage(id: messageID, workspaceID: thread.workspaceID, threadID: thread.id,
                author: "user", originDeviceID: thread.originDeviceID, text: text, createdAt: .now, state: .pending)
            let context = try taskContext(db, thread: thread, message: source, addressedAgentIDs: addressed.map(\.id))
            let agents = try workspace.agents.map { target in
                guard let project = workspace.projects.first(where: { $0.id == target.projectReferenceID }) else {
                    throw ThreadDeliveryError.targetChanged
                }
                return ThreadPlanAgent(id: target.id, name: target.name, project: project.name,
                    profile: target.profile.isEmpty ? "Agent" : target.profile)
            }
            let request = ThreadPlanRequest(turnID: UUID().uuidString, workspaceID: thread.workspaceID,
                threadID: thread.id, messageID: messageID, message: text, context: context.text, agents: agents)
            try request.validate()
            return PreparedThreadPlan(request: request, contextVersion: context.version, targets: workspace.agents,
                                      draftRevision: clearingDraftRevision)
        }
    }

    /// Commit the user message, complete Norma reply and all tasks together, or retain the original draft.
    public func applyNormaTurn(_ response: ThreadPlanResponse, prepared: PreparedThreadPlan) throws -> WorkThreadMessage {
        try prepared.request.validate()
        try response.validate(for: prepared.request)
        return try threadDatabase(writing: true) { db in
            let request = prepared.request
            if let prior = try readNormaPlan(db, id: request.turnID) {
                guard prior.prepared == prepared, prior.response == response else { throw WorkThreadError.conflict }
                return try deliveryMessage(db, id: request.messageID)
            }
            let thread = try requireThread(db, request.threadID)
            guard thread.workspaceID == request.workspaceID,
                  prepared.contextVersion > 0, prepared.contextVersion <= Int64.max,
                  prepared.targets.map(\.id) == request.agents.map(\.id),
                  try threadRows(db, "SELECT id,workspace_id,payload FROM work_messages WHERE id=?", [request.messageID]).isEmpty else {
                throw WorkThreadError.conflict
            }
            try requireWorkspace(db, request.workspaceID, writing: true)
            guard let row = try threadRows(db, "SELECT id,id,configuration FROM collaboration_workspaces WHERE id=?", [request.workspaceID]).first else {
                throw WorkThreadError.notFound
            }
            let workspace = try decode(CollaborationWorkspace.self, row)
            try validate(workspace)
            let source = try appendMessage(db, threadID: request.threadID, id: request.messageID, text: request.message,
                at: .now, clearingDraftRevision: prepared.draftRevision)
            var taskIDs: [String] = []
            for assignment in response.assignments {
                guard let target = prepared.targets.first(where: { $0.id == assignment.targetID }),
                      workspace.agents.contains(target) else { throw ThreadDeliveryError.targetChanged }
                let operation = try enqueueThreadTask(db, sourceMessageID: source.id, operationID: UUID().uuidString,
                    target: target, task: assignment.task, contextVersion: prepared.contextVersion, context: request.context)
                taskIDs.append(operation.id)
            }
            var reply = WorkThreadMessage(id: request.turnID, workspaceID: source.workspaceID, threadID: source.threadID,
                author: "norma", originDeviceID: source.originDeviceID, text: response.reply, createdAt: .now, state: .delivered)
            reply.replyToMessageID = source.id
            try insertThreadRecord(db, "INSERT INTO work_messages(id,workspace_id,thread_id,payload) VALUES(?,?,?,?)",
                                   [reply.id, reply.workspaceID, reply.threadID], reply)
            let record = ThreadPlanRecord(prepared: prepared, response: response, taskIDs: taskIDs)
            try insertThreadRecord(db, "INSERT INTO thread_norma_plans(id,message_id,thread_id,payload) VALUES(?,?,?,?)",
                                   [record.id, source.id, source.threadID], record)
            return source
        }
    }

    func readNormaPlan(_ db: OpaquePointer, id: String) throws -> ThreadPlanRecord? {
        guard let row = try threadRows(db, "SELECT id,thread_id,payload FROM thread_norma_plans WHERE id=?", [id]).first else { return nil }
        let record = try decode(ThreadPlanRecord.self, row)
        let request = record.prepared.request
        try request.validate(); try record.response.validate(for: request)
        let source = try deliveryMessage(db, id: request.messageID)
        guard row.scope == request.threadID, source.workspaceID == request.workspaceID, source.threadID == request.threadID,
              source.text == request.message, record.taskIDs.count == record.response.assignments.count,
              Set(record.taskIDs).count == record.taskIDs.count,
              record.prepared.targets.map(\.id) == request.agents.map(\.id),
              let replyRow = try threadRows(db, "SELECT id,workspace_id,payload FROM work_messages WHERE id=?", [id]).first else {
            throw WorkThreadError.damagedStore
        }
        let reply = try decode(WorkThreadMessage.self, replyRow)
        guard reply.author == "norma", reply.taskOperationID == nil, reply.state == .delivered,
              reply.replyToMessageID == source.id,
              reply.originDeviceID == source.originDeviceID, reply.threadID == request.threadID,
              reply.workspaceID == request.workspaceID, replyRow.scope == request.workspaceID,
              reply.text == record.response.reply else { throw WorkThreadError.damagedStore }
        for (operationID, assignment) in zip(record.taskIDs, record.response.assignments) {
            let task = try readTaskOutbox(db, operationID: operationID)
            guard task.task.sourceMessageID == request.messageID, task.target?.id == assignment.targetID,
                  task.task.task == assignment.task, task.task.context == request.context,
                  task.task.contextVersion == record.prepared.contextVersion else { throw WorkThreadError.damagedStore }
        }
        return record
    }
}

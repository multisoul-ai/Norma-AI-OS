import Foundation

extension CollaborationWorkspaceStore {
    public func saveThreadReviewDraft(_ draft: WorkThreadDraft?, operationID: String) throws {
        try threadDatabase(writing: true) { db in
            guard draft.map({ $0.valid && $0.mentions == nil && $0.followupTaskID == nil }) ?? true else { throw WorkThreadError.invalidContent }
            var original = try readTaskOutbox(db, operationID: operationID)
            original.reviewDraft = draft
            try writeTaskOutbox(db, original)
        }
    }

    public func requestThreadRevision(operationID: String, draft: WorkThreadDraft) throws -> ThreadTaskOutboxOperation {
        try threadDatabase(writing: true) { db in
            var original = try readTaskOutbox(db, operationID: operationID)
            if let existing = original.revisionOperationID { return try readTaskOutbox(db, operationID: existing) }
            guard original.acceptance == nil || (original.acceptance?.receipt?.state == "rejected" && original.acceptance?.acceptance == nil),
                  let delivery = original.receipt?.execution?.execution.delivery, delivery.available,
                  let taskRef = original.receipt?.receipt.result?.taskRef, let target = original.target,
                  draft.valid, draft.mentions == nil, draft.followupTaskID == nil, original.reviewDraft == draft,
                  !draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw WorkThreadError.conflict }
            try requireTaskTarget(db, original)
            let thread = try requireThread(db, original.task.threadID)
            let message = try appendMessage(db, threadID: thread.id, id: UUID().uuidString, text: draft.text, at: .now, clearingDraftRevision: nil)
            let context = try taskContext(db, thread: thread, message: message)
            guard var fields = try JSONSerialization.jsonObject(with: Data(context.text.utf8)) as? [String: Any] else { throw WorkThreadError.invalidContent }
            let reference = ThreadTaskRevision(operationID: original.id, taskRef: taskRef, fingerprint: delivery.fingerprint)
            fields["revision_of"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(reference))
            fields["original_task"] = original.task.task
            if let reply = original.receipt?.execution?.execution.result?.text { fields["original_reply"] = reply }
            let revision = try enqueueThreadTask(db, sourceMessageID: message.id, operationID: UUID().uuidString,
                target: target, task: "根据原成果修改：\n" + message.text, contextVersion: context.version,
                context: String(decoding: JSONSerialization.data(withJSONObject: fields, options: [.sortedKeys]), as: UTF8.self),
                revisionOf: reference)
            original.revisionOperationID = revision.id; original.reviewDraft = nil
            try writeTaskOutbox(db, original)
            return revision
        }
    }
}

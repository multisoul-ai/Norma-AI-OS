import Foundation
import NormaCore

extension VoiceCall {
    func loadOperations() async {
        do { operationRecords = try await operationStore.load() }
        catch { recoveryStatus = "未能读取操作记录，原文件已保留。请勿重复安排结果未明的任务。" }
    }

    func startOperationRecovery() {
        operationTask?.cancel()
        guard let target, wantsCall else { return }
        let current = generation
        operationTask = Task {
            await loadOperations()
            do {
                let client: RealtimeBindingClient
                if let cached = macBindingClient { client = cached }
                else { client = try await mobileClient.prepareRealtimeBinding(target.machine) }
                guard current == generation, wantsCall, !Task.isCancelled else { return }
                macBindingClient = client
                startReportObservation(client: client, target: target)
                var cursor: UInt64 = 0
                while !Task.isCancelled, wantsCall, current == generation {
                    let page = try await client.operations(workspaceID: target.project.id, after: cursor)
                    for var record in page {
                        record.targetName = target.machine.name + " · " + target.project.displayName
                        // Store the identity before reading its final receipt. Never submit from recovery.
                        if let index = operationRecords.firstIndex(where: { $0.id == record.id }) { operationRecords[index] = record }
                        else { operationRecords.append(record) }
                        try await operationStore.save(record)
                        if record.unresolved {
                            do {
                                let verified = try await client.receipt(for: record)
                                try await operationStore.save(verified)
                            } catch MobileError.commandNotFound {
                                // Reservation may precede dispatch, or Mac may have stopped between them.
                                // Retain its exact identity and unknown state; it is not safe to send again.
                            }
                        }
                        cursor = record.operation.sequence
                    }
                    try Task.checkCancellation()
                    operationRecords = try await operationStore.load()
                    recoveryStatus = nil
                    if page.count == 20 { continue }
                    // Reread from the oldest unresolved reservation to settle the Mac's frozen budget too.
                    if let first = operationRecords.filter({ $0.machineID == target.machine.id &&
                        $0.operation.workspaceID == target.project.id && $0.unresolved })
                        .map(\.operation.sequence).min() { cursor = min(cursor, first - 1) }
                    try await Task.sleep(for: .seconds(2))
                }
            } catch {
                guard current == generation, wantsCall, !Task.isCancelled else { return }
                recoveryStatus = "操作回执尚未核对。可继续聊天，请勿重复安排同一任务；重新连接后会继续查询。"
            }
        }
    }
}

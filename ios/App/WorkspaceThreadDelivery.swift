import Foundation
import NormaCore

func collaborationRelayClient() throws -> CollaborationRelayClient {
    if let local = try LocalCloudConfiguration.bundled() {
        #if DEBUG
        guard ProcessInfo.processInfo.environment["NORMA_CLOUD_LOCAL_PORT"] == nil else { throw CloudTextError.invalidRequest }
        #endif
        return CollaborationRelayClient(local: local)
    }
    #if DEBUG
    if let local = try CloudTextClient.developmentSettings(in: ProcessInfo.processInfo.environment) {
        return try CollaborationRelayClient(localPort: local.port)
    }
    #endif
    return CollaborationRelayClient()
}

extension WorkspaceLibraryModel {
    func beginTerminalSession(machineID: String) -> UUID {
        let id = UUID(); terminalSessions[machineID] = id; return id
    }

    func endTerminalSession(machineID: String, id: UUID) {
        if terminalSessions[machineID] == id { terminalSessions[machineID] = nil }
    }

    func refreshDelivery(machines: [PairedMachine], cloud: CloudChatModel, vault: CredentialVault) async {
        await cloud.load() // Failed initialization remains retryable on the next foreground pass.
        guard !Task.isCancelled else { return }
        guard cloud.loaded else {
            deliveryError = cloud.error ?? "暂时无法读取云端授权，恢复后会继续发送。"
            return
        }
        // Upgrade already-paired phones too; users of earlier builds should not scan a second code.
        guard !cloud.enrolling else { return }
        if cloud.credential == nil, let machine = machines.first {
            do { try await cloud.register(on: machine, using: MobileClient(vault: vault)) }
            catch is CancellationError { return }
            catch {
                cloud.error = error.localizedDescription
                deliveryError = "电脑已配对，手机注册尚未完成：\(error.localizedDescription)"
                return
            }
        }
        await deliverPending(machines: machines, cloud: cloud.credential, vault: vault)
    }

    /// RootView owns the foreground loop; cancellation leaves durable operations for the next run.
    func deliverPending(machines: [PairedMachine], cloud: CloudDeviceCredential?, vault: CredentialVault) async {
        guard !deliveryInProgress else { return }
        deliveryInProgress = true
        defer { deliveryInProgress = false }
        do {
            let pending = try await store.pendingThreadDeliveries()
            let pendingTasks = try await store.pendingThreadTasks(includingExecution: true)
            let pendingFollowups = try await store.pendingThreadFollowups()
            let pendingOperations = pendingTasks.map { (id: $0.id, machineID: $0.machineID) }
                + pendingFollowups.map { (id: $0.id, machineID: $0.machineID) }
            guard !pending.isEmpty || !pendingOperations.isEmpty || !deliveryErrors.isEmpty || !taskDeliveryErrors.isEmpty || deliveryError != nil else { return }
            defer { deliveryRevision &+= 1 }
            let pendingIDs = Set(pending.map(\.messageID))
            // A bounded batch must not erase the reason on messages it has not retried yet.
            var failures = deliveryErrors.filter { pendingIDs.contains($0.key) }
            let taskIDs = Set(pendingOperations.map(\.id))
            var taskFailures = taskDeliveryErrors.filter { taskIDs.contains($0.key) }
            let taskTargets = Dictionary(grouping: pendingOperations, by: \.machineID)
            var targets: [String: [String]] = [:]
            for item in pending {
                try Task.checkCancellation()
                do {
                    let prepared = try await store.prepareThreadDelivery(messageID: item.messageID)
                    if let mac = prepared.machineID { targets[mac, default: []].append(item.messageID) }
                } catch is CancellationError { throw CancellationError() }
                catch { failures[item.messageID] = error.localizedDescription }
            }
            let client = try collaborationRelayClient()
            for mac in Set(targets.keys).union(taskTargets.keys).sorted() {
                try Task.checkCancellation()
                guard terminalSessions[mac] == nil else { continue }
                deliveryMachineID = mac
                defer { deliveryMachineID = nil }
                do {
                    guard let cloud else { throw CloudTextError.unauthorized }
                    guard let machine = machines.first(where: { $0.id == mac }) else { throw MobileError.invalidCredential }
                    let connection = try await client.connect(machine, cloud: cloud, vault: vault)
                    do {
                        let hasTasks = !(taskTargets[mac] ?? []).isEmpty
                        // One message (2 calls) + one task OR follow-up (3 calls) + open/handshake/auth/close (4)
                        // <= 9 frames per 5s pass; 12 * 9 = 108 < the relay's 120/min ceiling.
                        let batch = try await store.deliverPendingThreadMessages(using: connection, limit: hasTasks ? 1 : 2)
                        failures.merge(batch) { _, new in new }
                        if hasTasks, let attempt = try await store.deliverPendingThreadTasks(using: connection) {
                            taskFailures[attempt.operationID] = attempt.error
                        }
                        await connection.close()
                    } catch {
                        await connection.close()
                        throw error
                    }
                } catch is CancellationError { throw CancellationError() }
                catch {
                    for id in targets[mac] ?? [] { failures[id] = error.localizedDescription }
                    for task in taskTargets[mac] ?? [] { taskFailures[task.id] = error.localizedDescription }
                }
            }
            let remainingIDs = Set(try await store.pendingThreadDeliveries().map(\.messageID))
            deliveryErrors = failures.filter { remainingIDs.contains($0.key) }
            let remainingTasks = Set(try await store.pendingThreadTasks(includingExecution: true).map(\.id))
            let remainingFollowups = Set(try await store.pendingThreadFollowups().map(\.id))
            taskDeliveryErrors = taskFailures.filter { remainingTasks.contains($0.key) || remainingFollowups.contains($0.key) }
            deliveryError = nil
        } catch is CancellationError { }
        catch { deliveryError = error.localizedDescription; deliveryRevision &+= 1 }
    }
}

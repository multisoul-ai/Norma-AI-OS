import SwiftUI
import NormaCore

/// One foreground connection owns reads and explicit human actions; saved actions are only queried on recovery.
@MainActor @Observable final class WorkThreadTerminalModel {
    var operation: ThreadTaskOutboxOperation
    var latest: ThreadTaskTerminal?
    var history: ThreadTaskTerminal?
    var draft: WorkThreadDraft?
    var error: String?
    var notice: String?
    var connected = false
    var reading = false
    private(set) var working = false
    private(set) var claimID: String?
    private var claimCandidate: String?
    private var connection: CollaborationRelayConnection?
    private var store: CollaborationWorkspaceStore?
    private var generation = UUID()
    private var work: Task<Void, Never>?
    private var draftWrite: Task<Void, Never>?
    private var dirtyDraft = false
    private var loaded = false
    private var backgroundWrite: UIBackgroundTaskIdentifier = .invalid

    init(operation: ThreadTaskOutboxOperation) { self.operation = operation }
    var displayed: ThreadTaskTerminal? { history ?? latest }
    var unresolved: Bool { operation.terminalOperation?.unresolved == true }
    var controlled: Bool { connected && claimID != nil }
    var canAct: Bool { connected && !working && !unresolved && latest != nil }
    var canInput: Bool { canAct && controlled && history == nil && latest?.page.inputReady == true }
    var canAcknowledge: Bool {
        unresolved && !working && (operation.terminalOperation?.receipt == nil || operation.terminalOperation?.receipt?.state == "outcome_unknown")
    }

    func run(library: WorkspaceLibraryModel, computers: ComputerModel, cloud: CloudChatModel) async {
        stop()
        let id = generation
        store = library.store; error = nil
        let reservation = library.beginTerminalSession(machineID: operation.machineID)
        var opened: CollaborationRelayConnection?
        do {
            operation = try await library.store.threadTask(operationID: operation.id)
            if !loaded { draft = operation.terminalDraft; loaded = true }
            if operation.terminalOperation?.outcomeAcknowledged == true {
                notice = "已记录你的核对；原回执和草稿仍保留。"
            }
            // A previously started background batch must finish before this Mac's terminal opens.
            while library.deliveryMachineID == operation.machineID {
                try await Task.sleep(for: .milliseconds(100))
            }
            try Task.checkCancellation()
            await cloud.load()
            guard let credential = cloud.credential else { throw CloudTextError.unauthorized }
            guard let machine = computers.machines.first(where: { $0.id == operation.machineID }) else { throw MobileError.invalidCredential }
            let session = try await collaborationRelayClient().connect(machine, cloud: credential, vault: computers.vault)
            opened = session
            try Task.checkCancellation()
            guard generation == id else { throw CancellationError() }
            connection = session; connected = true
            while !Task.isCancelled, generation == id, library.terminalSessions[operation.machineID] == reservation {
                if work == nil {
                    reading = true
                    do {
                        if unresolved { try await queryPending(session) }
                        try await readLatest(session)
                    } catch { reading = false; throw error }
                    reading = false
                }
                // 30 reads/minute, or at most 60 with unresolved-receipt queries, within 108 shared frames.
                // Each controlled read renews the Mac's 15-second lease. No input is replayed here.
                try await Task.sleep(for: .seconds(2))
            }
        } catch {
            if generation == id && !Task.isCancelled { self.error = error.localizedDescription }
        }
        await opened?.close()
        if generation == id { stop() }
        library.endTerminalSession(machineID: operation.machineID, id: reservation)
    }

    func stop() {
        generation = UUID(); notice = nil; connected = false; claimID = nil; claimCandidate = nil; history = nil
        work?.cancel(); work = nil; working = false; reading = false
        let old = connection; connection = nil
        Task { await old?.close() } // Closing the authenticated stream revokes even a pending claim.
        finishDraftWrites()
    }

    func claim() {
        guard canAct, !controlled else { return }
        perform { session in
            try await self.readLatest(session)
            let action = ThreadTerminalAction()
            self.claimCandidate = action.operationID
            try await self.submit(action, session: session)
            if self.claimID != nil { try await self.readLatest(session) }
        }
    }

    func release() {
        guard canAct, let claimID else { return }
        perform { session in
            try await self.submit(.release(claimOperationID: claimID), session: session)
            if self.claimID == nil { try await self.readLatest(session) }
        }
    }

    /// Native composition stays editable locally. Return/Tab/arrows commit text once, then their key.
    func send(_ key: TerminalInput) {
        guard canInput else { return }
        let captured = draft
        perform { session in
            await self.flushDraft()
            if let captured, !captured.text.isEmpty {
                guard self.draft?.revision == captured.revision else { throw WorkThreadError.conflict }
                try TerminalInput.text(captured.text).validate()
                try await self.store?.saveThreadTerminalDraft(captured, operationID: self.operation.id)
                try await self.readLatest(session)
                guard let claim = self.claimID, let page = self.latest?.page else { throw MobileError.terminalControlExpired }
                try await self.submit(try .init(claimOperationID: claim, page: page, input: .text(captured.text)),
                                      session: session, clearing: captured.revision)
                // A late/unknown text write never authorizes an automatic Return on recovery.
                guard self.operation.terminalOperation?.receipt?.state == "succeeded" else { return }
            }
            try Task.checkCancellation()
            try await self.readLatest(session)
            guard let claim = self.claimID, let page = self.latest?.page else { throw MobileError.terminalControlExpired }
            try await self.submit(try .init(claimOperationID: claim, page: page, input: key), session: session)
            try await self.readLatest(session)
        }
    }

    func loadOlder() {
        guard connected, !working, let previous = displayed, previous.page.startLine > 0 else { return }
        perform { session in
            if let claimID = self.claimID {
                self.history = try await session.controlledThreadTerminal(for: self.operation, claimOperationID: claimID, before: previous)
            } else {
                guard let receipt = self.operation.receipt else { throw MobileError.terminalUnavailable }
                self.history = try await session.threadTaskTerminal(task: self.operation.task, receipt: receipt, before: previous)
            }
        }
    }

    func acknowledge() async -> Bool {
        guard canAcknowledge, let state = operation.terminalOperation, let store else { return false }
        do {
            try await store.acknowledgeThreadTerminalOutcome(operationID: operation.id, controlOperationID: state.action.operationID)
            operation = try await store.threadTask(operationID: operation.id)
            stop() // Reconnect as a reader; a human acknowledgement never restores input ownership.
            notice = "已记录你的核对；原回执和草稿仍保留。"
            return true
        } catch { self.error = error.localizedDescription; return false }
    }

    private func perform(_ action: @escaping @MainActor (CollaborationRelayConnection) async throws -> Void) {
        guard !working, connected, let session = connection else { return }
        working = true; error = nil
        let id = generation
        work = Task {
            do {
                while reading { try await Task.sleep(for: .milliseconds(50)) }
                try Task.checkCancellation()
                try await action(session)
            } catch {
                await session.close()
                if generation == id { stop(); self.error = error.localizedDescription }
            }
            if generation == id { working = false; work = nil }
        }
    }

    private func readLatest(_ session: CollaborationRelayConnection) async throws {
        guard let receipt = operation.receipt else { throw MobileError.terminalUnavailable }
        let next: ThreadTaskTerminal
        if let claimID { next = try await session.controlledThreadTerminal(for: operation, claimOperationID: claimID) }
        else { next = try await session.threadTaskTerminal(task: operation.task, receipt: receipt) }
        try Task.checkCancellation()
        latest = next
    }

    private func submit(_ action: ThreadTerminalAction, session: CollaborationRelayConnection, clearing: String? = nil) async throws {
        guard let store, let latest else { throw MobileError.terminalUnavailable }
        operation = try await store.prepareThreadTerminalAction(operationID: operation.id, action: action, terminal: latest, clearingDraftRevision: clearing)
        let receipt = try await session.submitThreadTerminalAction(for: operation)
        try await apply(receipt)
        // Two receipt checks at 250ms/500ms bound foreground feedback; remaining checks use the 2s read loop.
        for delay in [250, 500] where operation.terminalOperation?.receipt?.state == "accepted" {
            try await Task.sleep(for: .milliseconds(delay))
            try await queryPending(session)
        }
    }

    private func queryPending(_ session: CollaborationRelayConnection) async throws {
        do { try await apply(session.threadTerminalReceipt(for: operation)) }
        catch MobileError.commandNotFound {
            notice = "Mac 没有这次操作的回执。请先在电脑核对，原输入不会自动重发。"
        }
    }

    private func apply(_ receipt: MobileCommandReceipt) async throws {
        guard let store else { throw MobileError.terminalUnavailable }
        await flushDraft()
        try await store.applyThreadTerminalReceipt(receipt, operationID: operation.id)
        let updated = try await store.threadTask(operationID: operation.id)
        try Task.checkCancellation()
        operation = updated
        guard let state = operation.terminalOperation else { return }
        if state.receipt?.state == "succeeded" {
            if let revision = state.draftRevision, draft?.revision == revision { draft = nil }
            switch state.action.type {
            case .claim:
                if claimCandidate == state.action.operationID { claimID = claimCandidate }
                notice = claimID == nil ? "上次接管已结束，需要重新接管。" : "已接管，Agent 自动输入已暂停。"
            case .input: notice = "已写入终端"
            case .release: claimID = nil; claimCandidate = nil; notice = "已交还 Agent"
            }
        } else if state.receipt?.state == "rejected" {
            error = state.receipt?.error == "terminal_changed" ? MobileError.terminalChanged.localizedDescription
                : "Mac 未执行这次操作（\(state.receipt?.error ?? "未知原因")）。草稿已保留。"
        } else { notice = "操作结果待核对，原输入不会自动重发。" }
    }

    func updateDraft(_ text: String) {
        guard loaded, text != (draft?.text ?? ""), let store else { return }
        draft = text.isEmpty ? nil : WorkThreadDraft(text: text)
        dirtyDraft = true
        guard draftWrite == nil else { return }
        draftWrite = Task {
            while dirtyDraft {
                dirtyDraft = false
                let captured = draft
                do { try await store.saveThreadTerminalDraft(captured, operationID: operation.id) }
                catch { self.error = "终端草稿尚未保存，请保留当前页面并重试。" }
            }
            draftWrite = nil
        }
    }
    private func flushDraft() async { while let write = draftWrite { await write.value } }
    private func finishDraftWrites() {
        guard draftWrite != nil, backgroundWrite == .invalid else { return }
        backgroundWrite = UIApplication.shared.beginBackgroundTask(withName: "Save terminal draft") { [weak self] in
            Task { @MainActor in self?.endDraftWrites() }
        }
        Task { await flushDraft(); endDraftWrites() }
    }
    private func endDraftWrites() {
        guard backgroundWrite != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundWrite); backgroundWrite = .invalid
    }
}

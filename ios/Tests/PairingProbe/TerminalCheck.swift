import Foundation
import NormaCore

enum TerminalCheck {
    /// 原生Swift客户端对原PTY粘贴→Enter→真实无工具回复；写入回执与Provider完成分开验证。
    /// 数据构造：1张原卡，固定无工具提示；1段文本和1次Enter；重复同操作仅查原回执。
    /// 执行过程：读首屏→订阅→写原文→等草稿重绘→Enter→原会话真实assistant→核对流/旧run。
    /// 预期结果：两次输入均written，实际对话仅一条user；原run保留；不能读取其他run。
    @MainActor static func run(client: MobileClient, machine: PairedMachine) async throws {
        let catalog = try await client.snapshot(machine)
        guard let workspace = catalog.projects.first?.id else { throw MobileError.invalidResponse }
        let snapshot = try await client.snapshot(machine, workspaceID: workspace)
        guard let project = snapshot.projects.first, let route = project.routeID, let card = project.cards?.first,
              let run = card.runID else { throw MobileError.invalidResponse }
        let scope = TerminalRequest(workspaceID: workspace, routeID: route, nodeID: card.id, runID: run, sessionID: card.sessionID)
        let observer = TerminalObserver(client: client, machine: machine, request: scope)
        defer { observer.task?.cancel() }
        let page = try await observer.require { $0.inputReady }
        let measuring = ProcessInfo.processInfo.environment["NORMA_STREAM_TIMING"] == "1"
        let text = measuring
            ? "Do not use any tools, skills, read or modify files. Output 120 numbered lines about rivers, each line one short sentence. End with NORMA_TERMINAL_READY. No preamble."
            : "Do not use any tools, read or modify files. Reply exactly NORMA_TERMINAL_READY"
        let paste = try await client.prepareTerminalInput(machine, workspaceID: workspace, routeID: route, revision: snapshot.grant.revision, page: page, input: .text(text))
        _ = try await client.submit(paste, to: machine)
        let receipt = try await MessageCheck.receipt(client, machine, paste)
        guard receipt.state == "succeeded", receipt.result?.status == "written", receipt.result?.turnID == nil else {
            print("TERMINAL_PASTE \(receipt.state) \(receipt.error ?? "none")"); throw MobileError.invalidResponse
        }
        let same = try await client.submit(paste, to: machine)
        guard same.operationID == paste.request.operationID, same.result?.status == "written" else { throw MobileError.invalidResponse }
        let draft = try await observer.require { $0.inputSequence > page.inputSequence && $0.rows.joined().contains("NORMA_TERMINAL_READY") }
        // Native CLI input handling matches the existing100ms paste-to-Enter separation.
        try await Task.sleep(for: .milliseconds(100))
        let fresh = try await client.terminal(machine, request: scope)
        guard fresh.revision >= draft.revision else { throw MobileError.invalidResponse }
        let enter = try await client.prepareTerminalInput(machine, workspaceID: workspace, routeID: route, revision: snapshot.grant.revision, page: fresh, input: .enter)
        _ = try await client.submit(enter, to: machine)
        let entered = try await MessageCheck.receipt(client, machine, enter)
        guard entered.state == "succeeded", entered.result?.status == "written" else {
            print("TERMINAL_ENTER \(entered.state) \(entered.error ?? "none")"); throw MobileError.invalidResponse
        }
        if measuring {
            try await measure(client, machine, workspace, route, card, observer)
            return
        }
        let completed = try await MessageCheck.completed(client, machine, workspace, route, card.id, "NORMA_TERMINAL_READY")
        guard completed.runID == run, let session = completed.sessionID else { throw MobileError.invalidResponse }
        let conversation = try await client.conversation(machine, request: ConversationRequest(workspaceID: workspace, routeID: route, nodeID: card.id, runID: run, sessionID: session, cursor: nil))
        guard conversation.page.items.filter({ $0.role == "user" }).map(\.text) == [text] else { throw MobileError.invalidResponse }
        _ = try await observer.require { $0.rows.joined().contains("NORMA_TERMINAL_READY") && $0.inputSequence > fresh.inputSequence }
        guard observer.changes > 1 else { throw MobileError.invalidResponse }
        let wrong = TerminalRequest(workspaceID: workspace, routeID: route, nodeID: card.id, runID: "stale-run", sessionID: session)
        do { _ = try await client.terminal(machine, request: wrong); throw MobileError.invalidResponse }
        catch MobileError.sessionChanged {}
        observer.task?.cancel(); await observer.task?.value
        print("NATIVE_REAL_TERMINAL_OK \(card.profile) changes=\(observer.changes)")
    }

    /// 原TUI长回复对比终端更新和完整消息入档时刻，不把合成数据当模型增量。
    /// 数据构造：120行无工具文本；90s=模型预算75s+历史同步15s，仅本隔离进程。
    /// 执行过程：持续接收PTY → 原session出现后订阅历史 → 等待真实assistant结束标记。
    /// 预期结果：记录每次正文长度/终端版本；必须有真实正文，不能用输入回显作回复。
    @MainActor private static func measure(_ client: MobileClient, _ machine: PairedMachine, _ workspace: String,
                                          _ route: String, _ card: CardSnapshot, _ terminal: TerminalObserver) async throws {
        let started = ContinuousClock.now
        let deadline = started.advanced(by: .seconds(90))
        var watch: ConversationWatchCheck?
        defer { watch?.cancel() }
        var counts: [Int] = []
        while ContinuousClock.now < deadline {
            if watch == nil {
                let snapshot = try await client.snapshot(machine, workspaceID: workspace)
                if let current = snapshot.projects.first?.cards?.first(where: { $0.id == card.id }),
                   let run = current.runID, let session = current.sessionID {
                    watch = ConversationWatchCheck(client: client, machine: machine,
                        request: ConversationRequest(workspaceID: workspace, routeID: route, nodeID: card.id, runID: run, sessionID: session, cursor: nil))
                }
            }
            let replies = watch?.timeline.items.filter { $0.role == "assistant" }.map(\.text) ?? []
            let count = replies.joined().count
            if counts.last != count {
                counts.append(count)
                print("NATIVE_STREAM_SAMPLE provider=\(card.profile) elapsed=\(started.duration(to: .now)) chars=\(count) ptyChanges=\(terminal.changes)")
            }
            if replies.contains(where: { $0.contains("NORMA_TERMINAL_READY") }) {
                print("NATIVE_REAL_TERMINAL_OK \(card.profile) assistantLengths=\(counts) ptyChanges=\(terminal.changes)")
                return
            }
            try await Task.sleep(for: .milliseconds(50))
        }
        throw MobileError.timedOut
    }
}

@MainActor private final class TerminalObserver {
    var page: TerminalPage?
    var changes = 0
    var failure: (any Error)?
    var task: Task<Void, Never>?
    init(client: MobileClient, machine: PairedMachine, request: TerminalRequest) {
        task = Task {
            do {
                for try await update in client.terminalUpdates(machine, request: request) {
                    switch update {
                    case let .current(value): page = value; changes += 1
                    case .recovering: page = nil
                    case .heartbeat: break
                    }
                }
            } catch { if !Task.isCancelled { failure = error } }
        }
    }
    func require(_ matches: (TerminalPage) -> Bool) async throws -> TerminalPage {
        let deadline = ContinuousClock.now.advanced(by: .seconds(15)) //读取5s+原CLI重绘/调度10s，有限等待。
        while ContinuousClock.now < deadline {
            if let failure { throw failure }
            if let page, matches(page) { return page }
            try await Task.sleep(for: .milliseconds(50))
        }
        throw MobileError.timedOut
    }
}

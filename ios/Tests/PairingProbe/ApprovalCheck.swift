import Foundation
import NormaCore

enum ApprovalCheck {
    /// Real Provider, exact temporary writes only. The Rust host also checks final disk effects.
    /// Data: two explicit target paths supplied beside the isolated QR, never arbitrary tool input.
    /// Steps: send → pending → persist deny/allow → query after lost response → finish → duplicate.
    /// Expected: no pre-approval writes, denied absent, allowed exact content, distinct request IDs.
    @MainActor static func run(client: MobileClient, machine: PairedMachine, directory: URL, external: Bool = false, measuring: Bool = false, bashEdges: Bool = false, stopFirst: Bool = false, writeTool: Bool = false) async throws {
        let targets = try JSONDecoder().decode([String].self, from: Data(contentsOf: directory.appendingPathComponent("approval-targets.json")))
        guard targets == ["denied.txt", "allowed.txt"].map({ directory.appendingPathComponent($0).path }),
              !directory.path.contains("'"), !directory.path.contains(" ") else { throw MobileError.invalidResponse }
        let catalog = try await client.snapshot(machine)
        guard let workspace = catalog.projects.first?.id, catalog.projects.count == 1 else { throw MobileError.invalidResponse }
        let first = try await client.snapshot(machine, workspaceID: workspace)
        guard let project = first.projects.first, let route = project.routeID, let empty = project.cards?.first else { throw MobileError.invalidResponse }
        let request = ApprovalListRequest(workspaceID: workspace, routeID: route)
        let watch = ApprovalWatchCheck(client: client, machine: machine)
        defer { watch.cancel() }
        try await watch.require(request)
        let store = PendingCommandStore(url: directory.appendingPathComponent("approval-decisions.json"))
        var card = empty
        var priorRequest: String?
        var prompts: [String] = []
        var submitted: [Duration] = [], confirmed: [Duration] = []
        for (index, decision) in [ApprovalDecision.deny, .allow].enumerated() {
            let longBash = bashEdges && decision == .allow
            let command = (longBash ? "sleep 30; " : "") + "printf hello-from-norma > " + targets[index] + (longBash ? "; exit 7" : "")
            // 30000ms is Codex's maximum initial wait; the 30s command must outlive the 20s approval receipt budget.
            let foreground = !longBash ? "" : card.profile == "Codex"
                ? " Set yield_time_ms to 30000. If a running session is returned, poll only that session using write_stdin until it exits."
                : " Set timeout to 60000ms and run_in_background to false."
            let tool = card.profile == "Codex" ? "exec_command" : "Bash"
            let permission = card.profile == "Codex" ? " Set sandbox_permissions to require_escalated and justification to 'Write the isolated Norma test marker?'." : ""
            let toolRestriction = longBash && card.profile == "Codex" ? "use other tools except write_stdin for that session" : "use other tools"
            let prompt = writeTool
                ? "This is an I/O test in a disposable directory. Use Write exactly once with file_path exactly \(targets[index]) and content exactly hello-from-norma with no newline. Request approval. Do not use Bash or other tools, read or modify other files or delegate. If denied, do not retry. Reply briefly and stop."
                : "This is an I/O test in a disposable directory. Use \(tool) exactly once: \(command)\nRequest approval for this exact write.\(foreground)\(permission) Do not read or modify other files, \(toolRestriction) or delegate. If denied, do not retry. Reply briefly and stop."
            prompts.append(prompt)
            let message = try await client.prepareMessage(machine, workspaceID: workspace, routeID: route, revision: first.grant.revision, card: card, text: prompt)
            _ = try await client.submit(message, to: machine)
            let sent = try await MessageCheck.receipt(client, machine, message)
            guard sent.state == "succeeded", let turn = sent.result?.turnID else { throw MobileError.invalidResponse }
            print("APPROVAL_TASK_SENT \(card.profile) \(decision.rawValue)")
            let approval = try await pending(client, machine, request, external: external, watch: watch)
            if external {
                print("EXTERNAL_NOTICE_READ desktop=\(approval.requiresDesktop) decisions=\(approval.decisions.count) node=\(approval.target.nodeID == card.id) turn=\(approval.target.turnID == turn)")
                guard approval.requiresDesktop, approval.decisions.isEmpty, approval.target.nodeID == card.id,
                      approval.target.turnID == turn, case .object(let notice) = approval.details,
                      notice["sourcePath"] == nil, notice["command"] == nil else { throw MobileError.invalidResponse }
                do {
                    _ = try await client.prepareApproval(machine, workspaceID: workspace, routeID: route, revision: first.grant.revision, snapshot: approval, decision: .allow)
                    throw MobileError.decisionUnavailable
                } catch MobileError.invalidResponse {}
                print("EXTERNAL_NOTICE_HAS_NO_ACTION")
                let current = try await client.snapshot(machine, workspaceID: workspace)
                guard let waiting = current.projects.first?.cards?.first else { throw MobileError.invalidResponse }
                let stop = try await client.prepareInterrupt(machine, workspaceID: workspace, routeID: route, revision: current.grant.revision, card: waiting)
                _ = try await client.submit(stop, to: machine)
                let stopped = try await MessageCheck.receipt(client, machine, stop)
                print("EXTERNAL_STOP \(stopped.state) \(stopped.result?.status ?? stopped.error ?? "none") turn=\(stopped.result?.turnID == turn)")
                guard stopped.state == "succeeded", stopped.result?.status == "interrupted", stopped.result?.turnID == turn,
                      !FileManager.default.fileExists(atPath: targets[0]) else { throw MobileError.invalidResponse }
                print("NATIVE_REAL_EXTERNAL_NOTICE_OK")
                return
            }
            let matchesInput: Bool
            if writeTool {
                matchesInput = approval.tool == "Write" && approval.details == .object(["file_path": .text(targets[index]), "content": .text("hello-from-norma")])
            } else if case .object(let input) = approval.details, case .text(let actual) = input["command"] {
                matchesInput = [command, "/bin/zsh -lc '\(command)'", "/bin/bash -lc '\(command)'"].contains(actual)
                    && approval.tool == (card.profile == "Codex" && !approval.target.requestID.hasPrefix("native-") ? "item/commandExecution/requestApproval" : "Bash")
            } else { matchesInput = false }
            guard matchesInput,
                  approval.target.nodeID == card.id, approval.target.turnID == turn,
                  approval.target.requestID != priorRequest, !FileManager.default.fileExists(atPath: targets[index]) else {
                // Unexpected input never receives approval; fixture teardown stops the Provider.
                throw MobileError.invalidResponse
            }
            let chosen = try await client.prepareApproval(machine, workspaceID: workspace, routeID: route, revision: first.grant.revision, snapshot: approval, decision: decision)
            if stopFirst && index == 0 {
                let current = try await client.snapshot(machine, workspaceID: workspace)
                guard let waiting = current.projects.first?.cards?.first,
                      waiting.turnID == turn, waiting.state == "waiting_approval" else { throw MobileError.invalidResponse }
                let stop = try await client.prepareInterrupt(machine, workspaceID: workspace, routeID: route, revision: current.grant.revision, card: waiting)
                try await store.save(stop)
                _ = try await client.submit(stop, to: machine)
                let restored = try await PendingCommandStore(url: directory.appendingPathComponent("approval-decisions.json")).load()
                guard let saved = restored.first(where: { $0.id == stop.id }) else { throw MobileError.invalidResponse }
                let stopped = try await MessageCheck.receipt(client, machine, saved)
                print("PENDING_STOP_RECEIPT \(card.profile) \(stopped.state) \(stopped.result?.status ?? stopped.error ?? "none")")
                guard stopped.state == "succeeded", stopped.result?.status == "interrupted",
                      stopped.result?.turnID == turn, !FileManager.default.fileExists(atPath: targets[0]) else { throw MobileError.invalidResponse }
                let replay = try await client.submit(saved, to: machine)
                guard replay.result?.turnID == turn, replay.result?.status == "interrupted" else { throw MobileError.invalidResponse }
                card = try await finished(client, machine, workspace, card.id)
                try await watch.require(request, removed: approval.target)
                let late = try await client.prepareApproval(machine, workspaceID: workspace, routeID: route, revision: first.grant.revision, snapshot: approval, decision: .allow)
                _ = try await client.submit(late, to: machine)
                let rejected = try await MessageCheck.receipt(client, machine, late)
                guard rejected.state == "rejected", rejected.error == "approval_changed" else { throw MobileError.invalidResponse }
                priorRequest = approval.target.requestID
                print("NATIVE_PENDING_STOP_REPLAY_AND_LATE_APPROVAL_OK \(card.profile)")
                continue
            }
            if measuring && index == 0 {
                var samples: [Duration] = []
                //20 reads of the same real pending request; no extra model turn or permission response.
                for _ in 0..<20 {
                    let started = ContinuousClock.now
                    let reread = try await client.approvals(machine, request: request)
                    let duration = ContinuousClock.now - started
                    guard reread == [approval] else { throw MobileError.invalidResponse }
                    samples.append(duration)
                }
                try NativeTiming.report("approval_list_read", provider: card.profile, samples: samples,
                    scope: "native Keychain + new TLS/WSS + authorized real pending request + decode; no model generation")
            }
            let decisionStarted = ContinuousClock.now
            try await store.save(chosen)
            _ = try await client.submit(chosen, to: machine) // Lose response; restore original operation.
            submitted.append(ContinuousClock.now - decisionStarted)
            let restored = try await PendingCommandStore(url: directory.appendingPathComponent("approval-decisions.json")).load()
            guard let saved = restored.first(where: { $0.id == chosen.id }), saved.approvalPreview == approval else { throw MobileError.invalidResponse }
            let result = try await MessageCheck.receipt(client, machine, saved)
            print("APPROVAL_RECEIPT \(card.profile) \(result.state) \(result.result?.status ?? result.error ?? "none")")
            guard result.state == "succeeded", result.result?.status == "submitted" else { throw MobileError.invalidResponse }
            confirmed.append(ContinuousClock.now - decisionStarted)
            if longBash {
                // 20s is the production confirmation ceiling; Bash writes only after 30s.
                guard ContinuousClock.now - decisionStarted < .seconds(20),
                      !FileManager.default.fileExists(atPath: targets[1]) else { throw MobileError.invalidResponse }
                print("NATIVE_LONG_BASH_RECEIPT_BEFORE_COMPLETION elapsed=\(ContinuousClock.now - decisionStarted)")
            }
            card = try await finished(client, machine, workspace, card.id, budget: longBash ? .seconds(45) : .seconds(30))
            try await watch.require(request, removed: approval.target)
            if decision == .deny {
                guard !FileManager.default.fileExists(atPath: targets[0]) else { throw MobileError.invalidResponse }
            } else {
                guard try String(contentsOfFile: targets[1], encoding: .utf8) == "hello-from-norma",
                      !FileManager.default.fileExists(atPath: targets[0]) else { throw MobileError.invalidResponse }
            }
            let replay = try await client.submit(saved, to: machine)
            guard replay.result?.requestID == approval.target.requestID, replay.result?.decision == decision else { throw MobileError.invalidResponse }
            let stale = try await client.prepareApproval(machine, workspaceID: workspace, routeID: route, revision: first.grant.revision, snapshot: approval, decision: .allow)
            _ = try await client.submit(stale, to: machine)
            let rejected = try await MessageCheck.receipt(client, machine, stale)
            guard rejected.state == "rejected", rejected.error == "approval_changed" else { throw MobileError.invalidResponse }
            priorRequest = approval.target.requestID
        }
        guard let run = card.runID, let session = card.sessionID else { throw MobileError.invalidResponse }
        let page = try await client.conversation(machine, request: ConversationRequest(workspaceID: workspace, routeID: route, nodeID: card.id, runID: run, sessionID: session, cursor: nil)).page
        // Native No appends an explicit interrupted marker as a synthetic user record.
        // Verify both actual prompts exactly once; do not confuse that marker with a third task.
        let users = page.items.filter { $0.role == "user" }.map(\.text)
        if bashEdges && card.profile == "Codex" {
            for item in page.items where item.role == "tool" { print("NATIVE_LONG_BASH_TOOL \(item.details ?? "missing")") }
            let tools = try page.items.filter { $0.role == "tool" }.compactMap { item -> [String: Any]? in
                guard let details = item.details else { return nil }
                return try JSONSerialization.jsonObject(with: Data(details.utf8)) as? [String: Any]
            }
            guard tools.contains(where: { $0["type"] as? String == "commandExecution" && $0["exitCode"] as? Int == 7 }) else { throw MobileError.invalidResponse }
            print("NATIVE_LONG_BASH_EXIT_CODE_OK 7")
        }
        print("APPROVAL_HISTORY users=\(users.count) first=\(users.filter { $0 == prompts[0] }.count) second=\(users.filter { $0 == prompts[1] }.count)")
        guard users.filter({ $0 == prompts[0] }).count == 1,
              users.filter({ $0 == prompts[1] }).count == 1,
              users.allSatisfy({ prompts.contains($0) || $0 == "[Request interrupted by user for tool use]" }),
              page.items.contains(where: { $0.role == "assistant" && !$0.text.isEmpty }),
              try await client.approvals(machine, request: request).isEmpty else { throw MobileError.invalidResponse }
        print("NATIVE_REAL_APPROVALS_OK \(card.profile)")
        await watch.finish()
        print("NATIVE_REAL_APPROVAL_FEED_OK \(card.profile)")
        if measuring {
            try NativeTiming.report("approval_request_ack", provider: card.profile, samples: submitted,
                scope: "2 real decisions, local durable save + new TLS/WSS + request acceptance; not Provider execution")
            try NativeTiming.report("approval_confirmed_receipt", provider: card.profile, samples: confirmed,
                scope: "2 real decisions, local save/restore + submit + durable submitted receipt; includes100ms query polling and new connections; not tool completion")
        }
    }

    @MainActor static func pending(_ client: MobileClient, _ machine: PairedMachine, _ request: ApprovalListRequest, external: Bool, watch: ApprovalWatchCheck) async throws -> ApprovalSnapshot {
        // 30s bounds the tiny model task before permission; approval keeps the Provider's own TTL.
        let deadline = ContinuousClock.now.advanced(by: .seconds(30))
        while ContinuousClock.now < deadline {
            let list = try watch.items(request) ?? []
            if !list.isEmpty { print("APPROVAL_LIST_COUNT \(list.count)") }
            // The production host may run several external Hooks for one permission request.
            // All must remain informational; the single-invocation allow/deny test stays exact.
            if external, !list.isEmpty, list.allSatisfy(\.requiresDesktop) { return list[0] }
            // Native Hooks precede the CLI dialog; wait for its exact actionable snapshot.
            if list.count == 1, !list[0].requiresDesktop { return list[0] }
            if list.count == 1, list[0].requiresDesktop {
                try await Task.sleep(for: .milliseconds(100))
                continue
            }
            guard list.isEmpty else { throw MobileError.invalidResponse }
            try await Task.sleep(for: .milliseconds(100))
        }
        let snapshot = try await client.snapshot(machine, workspaceID: request.workspaceID)
        if let card = snapshot.projects.first?.cards?.first, let run = card.runID, let session = card.sessionID {
            print("APPROVAL_MISSING_STATE \(card.profile) \(card.state)")
            let page = try await client.conversation(machine, request: ConversationRequest(workspaceID: request.workspaceID, routeID: request.routeID, nodeID: card.id, runID: run, sessionID: session, cursor: nil)).page
            for message in page.items where message.role == "assistant" { print("APPROVAL_MISSING_REPLY \(message.text)") }
        }
        throw MobileError.timedOut
    }
    static func finished(_ client: MobileClient, _ machine: PairedMachine, _ workspace: String, _ node: UInt64, budget: Duration = .seconds(30)) async throws -> CardSnapshot {
        // Ordinary tool budget30s; slow fixture45s=30s command +15s result/model margin.
        let deadline = ContinuousClock.now.advanced(by: budget)
        while ContinuousClock.now < deadline {
            let snapshot = try await client.snapshot(machine, workspaceID: workspace)
            guard let card = snapshot.projects.first?.cards?.first(where: { $0.id == node }) else { throw MobileError.invalidResponse }
            if ["done", "ended"].contains(card.state) { return card }
            guard card.state != "failed" else { throw MobileError.invalidResponse }
            try await Task.sleep(for: .milliseconds(100))
        }
        throw MobileError.timedOut
    }
}

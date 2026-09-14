import SwiftUI
import NormaCore

private struct ApprovalEntry: Identifiable, Sendable {
    let machine: PairedMachine
    let project: ProjectSnapshot
    let revision: UInt64
    let canDecide: Bool
    let snapshot: ApprovalSnapshot
    var id: String { [machine.id, project.id, project.routeID ?? "", String(snapshot.target.nodeID), snapshot.target.runID, snapshot.target.sessionID, snapshot.target.turnID, snapshot.target.requestID].joined(separator: "|") }
}
private struct ApprovalSource: Sendable {
    var items: [ApprovalEntry] = []
    var issues: [String] = []
}

struct CommandDecisionInbox: View {
    @Bindable var model: ComputerModel
    let manageComputers: () -> Void
    var active = true
    var focus: PushNotificationTarget? = nil
    @State private var focusedEvent: String?
    @Environment(\.scenePhase) private var scenePhase
    @State private var sources: [String: ApprovalSource] = [:]
    @State private var refresh = 0
    @State private var generation = 0
    @State private var records = false
    @State private var showing = false
    @State private var deciding: String?
    @State private var receipt: PendingMobileCommand?
    @State private var task: ApprovalEntry?
    @State private var error: String?
    @State private var selected: String?
    private var items: [ApprovalEntry] {
        model.machines.flatMap { sources[$0.id]?.items ?? [] }.filter { item in
            !model.pendingCommands.contains { $0.matchesApproval(machineID: item.machine.id, workspaceID: item.project.id, routeID: item.project.routeID ?? "", target: item.snapshot.target) }
        }
    }
    private var operationID: String { "\(scenePhase)-\(refresh)-\(active)-" + model.machines.map { $0.id + $0.endpoints.map(\.absoluteString).joined() }.joined() }
    private var decisionCount: Int? {
        guard model.machines.allSatisfy({ sources[$0.id]?.issues.isEmpty == true }) else { return nil }
        return items.count
    }

    var body: some View {
        Button { showing = true } label: {
            Label("待决定 · " + (decisionCount.map(String.init) ?? "待核验"), systemImage: "checkmark.shield")
                .font(.caption).frame(maxWidth: .infinity, minHeight: 44)
                .background(WorkspaceStyle.surface, in: Capsule())
        }.accessibilityLabel("待决定，全部电脑").accessibilityValue(decisionCount.map(String.init) ?? "待核验")
            .accessibilityIdentifier("command-decision-inbox")
            .sheet(isPresented: $showing) {
                NavigationStack {
                    decisionContent
                        .navigationTitle("待决定 · 全部电脑").navigationBarTitleDisplayMode(.inline)
                        .toolbar {
                            ToolbarItem(placement: .cancellationAction) {
                                Button("完成") { showing = false }.accessibilityIdentifier("command-decisions-close")
                            }
                            ToolbarItem(placement: .topBarTrailing) {
                                Button { records = true } label: { Image(systemName: "clock") }
                                    .accessibilityLabel("决定记录").accessibilityIdentifier("approval-records")
                            }
                        }
                        .sheet(isPresented: $records) { ApprovalRecordsView(model: model) }
                        .sheet(item: $task) { entry in ApprovalConversationView(entry: entry, model: model) { task = nil } }
                        .sheet(item: $receipt) { command in
                            NavigationStack {
                                ApprovalRecordView(model: model, command: command)
                                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("关闭") { receipt = nil } } }
                            }
                        }
                        .alert("决定暂未完成", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) {
                            Button("好") { error = nil }
                        } message: { Text(error ?? "") }
                }.preferredColorScheme(.dark).tint(.white)
            }
            .onChange(of: focus?.id, initial: true) { _, id in if id != nil { showing = true } }
            .task(id: operationID) { await observe() }
            .onDisappear { generation += 1; sources = [:] }
    }

    private var decisionContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let focus, sources.count == model.machines.count,
               !items.contains(where: { $0.machine.id == focus.machineID && $0.project.id == focus.workspaceID && $0.project.routeID == focus.routeID && focus.matches($0.snapshot) }) {
                Text("这项提醒已不在待确认列表，以下是当前事项。").font(.caption).foregroundStyle(.secondary).padding(.horizontal, 20)
            }
            if model.machines.isEmpty {
                WorkspaceStatusView(title: "先连接你的电脑", icon: "laptopcomputer.and.iphone", message: "Agent 负责执行，你在这里确认决策、调整方向。") {
                    WorkspaceStatusButton(title: "连接电脑", action: manageComputers)
                }
            } else {
                if items.isEmpty {
                    WorkspaceStatusView(title: decisionCount == nil ? "待确认事项尚未核验" : "当前没有可处理的事项", icon: decisionCount == nil ? "wifi.exclamationmark" : "checkmark.bubble", message: "决定记录保存在这台 iPhone，可随时回查。连接异常的电脑需重新核验。") {
                        if decisionCount == nil { WorkspaceStatusButton(title: "重新核验") { refresh += 1 } }
                    }
                } else {
                    Group {
                        if let item = items.first(where: { $0.id == selected }) ?? items.first {
                            ScrollView {
                                VStack(alignment: .leading, spacing: 20) {
                                    Text(item.machine.name + " · " + item.project.displayName).font(.caption).foregroundStyle(.secondary)
                                    ApprovalContext(snapshot: item.snapshot)
                                    if !item.canDecide { Text("此项目为只读").foregroundStyle(.secondary) }
                                    if item.snapshot.requiresDesktop { Text("请在电脑的 Agent 卡片中确认").foregroundStyle(.orange) }
                                    ForEach(item.snapshot.decisions, id: \.self) { decision in
                                        Button(decision == .allow ? "允许一次" : "拒绝本次", role: decision == .deny ? .destructive : nil) {
                                            Task { await decide(item, decision: decision) }
                                        }.frame(maxWidth: .infinity, minHeight: 44).buttonStyle(.bordered)
                                            .disabled(!canDecide(item)).accessibilityIdentifier(decision == .allow ? "approval-allow" : "approval-deny")
                                    }
                                    Button("查看原任务") { if deciding == nil { task = item } }.frame(minHeight: 44)
                                    if items.count > 1 {
                                        Button("下一项") {
                                            guard let index = items.firstIndex(where: { $0.id == item.id }) else { return }
                                            selected = items[(index + 1) % items.count].id
                                        }.frame(minHeight: 44).disabled(deciding != nil)
                                    }
                                    if deciding == item.id { ProgressView("正在提交决定…") }
                                }.padding(20)
                            }.id(item.id)
                        }
                    }
                    .onChange(of: items.map(\.id), initial: true) { _, ids in
                        if let focus, focusedEvent != focus.id,
                           let item = items.first(where: { $0.machine.id == focus.machineID && $0.project.id == focus.workspaceID && $0.project.routeID == focus.routeID && focus.matches($0.snapshot) }) {
                            selected = item.id; focusedEvent = focus.id
                        } else if !ids.contains(selected ?? "") { selected = ids.first }
                    }
                }
            }
        }
        .background(.black)
    }

    private func canDecide(_ item: ApprovalEntry) -> Bool {
        deciding == nil && active && scenePhase == .active && showing && !records && receipt == nil && task == nil && error == nil &&
            item.canDecide && !item.snapshot.requiresDesktop && (try? item.snapshot.details.formatted()) != nil
    }

    private func isCurrent(_ item: ApprovalEntry) -> Bool {
        (selected == nil || selected == item.id) && items.contains {
            $0.id == item.id && $0.revision == item.revision && $0.canDecide && $0.snapshot == item.snapshot
        }
    }

    @MainActor private func decide(_ item: ApprovalEntry, decision: ApprovalDecision) async {
        guard canDecide(item), isCurrent(item), item.snapshot.decisions.contains(decision),
              let route = item.project.routeID else { return }
        deciding = item.id
        defer { deciding = nil }
        do {
            let command = try await model.client.prepareApproval(item.machine, workspaceID: item.project.id, routeID: route, revision: item.revision, snapshot: item.snapshot, decision: decision)
            guard !Task.isCancelled, scenePhase == .active, showing,
                  !records, receipt == nil, task == nil, isCurrent(item) else { return }
            try await model.commandStore.save(command)
            // Save before send. Even if submission fails, this exact operation stays queryable.
            try await model.reloadCommands()
            _ = try await model.client.submit(command, to: item.machine)
            receipt = command
        } catch {
            self.error = error.localizedDescription + "。已保存的决定可在决定记录中查询。"
        }
    }

    @MainActor private func observe() async {
        generation += 1
        let current = generation
        guard scenePhase == .active && active else { sources = [:]; return }
        let machines = model.machines
        sources = sources.filter { id, _ in machines.contains { $0.id == id } }
        let client = model.client
        // One bounded WSS projection per computer. An offline Mac must not hold up another Mac.
        await withTaskGroup(of: Void.self) { group in
            for machine in machines {
                group.addTask { await observe(machine, client: client, generation: current) }
            }
        }
    }

    @MainActor private func observe(_ machine: PairedMachine, client: MobileClient, generation current: Int) async {
        do {
            for try await update in client.approvalUpdates(machine) {
                guard !Task.isCancelled, current == generation else { return }
                var source = ApprovalSource()
                switch update {
                case .recovering(let reason): source.issues = [reason]
                case .current(let snapshot):
                    for entry in snapshot.projects {
                        let project = entry.project
                        if project.state == "unavailable" || entry.error != nil {
                            let reason = entry.error == "approval_too_large" ? MobileError.approvalTooLarge.localizedDescription : "审批暂时无法读取，请稍后重试。"
                            source.issues.append(project.displayName + "：" + reason)
                        }
                        let control = snapshot.grant.projects.contains { $0.id == project.id && $0.access == "control" }
                        source.items += entry.items.map { ApprovalEntry(machine: machine, project: project, revision: snapshot.grant.revision, canDecide: control, snapshot: $0) }
                    }
                }
                sources[machine.id] = source
            }
        } catch {
            guard !Task.isCancelled, current == generation else { return }
            sources[machine.id] = ApprovalSource(issues: [error.localizedDescription])
        }
    }
}

/// Resolve the captured decision's original card once; ConversationView owns the live session thereafter.
private struct ApprovalConversationView: View {
    let entry: ApprovalEntry
    let model: ComputerModel
    let close: () -> Void
    @Environment(\.scenePhase) private var phase
    @State private var card: CardSnapshot?
    @State private var error: String?
    @State private var retry = 0
    private var paired: Bool { model.machines.contains { $0.id == entry.machine.id } }

    var body: some View {
        NavigationStack {
            Group {
                if paired, let card, let route = entry.project.routeID {
                    ConversationView(machine: entry.machine, workspaceID: entry.project.id, routeID: route, card: card, model: model)
                } else if let error = paired ? error : MobileError.wrongComputer.localizedDescription {
                    VStack {
                        ContentUnavailableView("无法打开原任务", systemImage: "bubble.left.and.exclamationmark.bubble.right",
                            description: Text(error))
                        Button("重新核验") { retry += 1 }.frame(minHeight: 44).disabled(!paired)
                    }
                } else { WorkspaceLoadingView(message: "正在读取原任务…") }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity).background(.black)
            .toolbar { ToolbarItem(placement: .cancellationAction) {
                Button("关闭", action: close).accessibilityIdentifier("approval-task-close")
            } }
            .task(id: "\(phase)-\(retry)-\(paired)") {
                guard phase == .active, card == nil else { return }
                error = nil
                do {
                    guard paired, let route = entry.project.routeID else { throw MobileError.wrongComputer }
                    let snapshot = try await model.client.snapshot(entry.machine, workspaceID: entry.project.id)
                    try Task.checkCancellation()
                    guard paired, let project = snapshot.projects.first else { throw MobileError.wrongComputer }
                    card = try entry.snapshot.target.card(in: project, workspaceID: entry.project.id, routeID: route)
                } catch { if !Task.isCancelled { self.error = error.localizedDescription } }
            }
        }.preferredColorScheme(.dark).tint(.white)
    }
}

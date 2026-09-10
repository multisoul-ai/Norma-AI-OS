import SwiftUI
import NormaCore

struct ProjectTodoView: View {
    let model: ComputerModel
    let machine: PairedMachine
    let plan: ProjectPlan
    var dashboard = false
    var active = true
    var liveProgress: ProjectPlanProgress? = nil
    var liveRevision = 0
    var adjustDirection: (() -> Void)? = nil
    var focusedTodoIndex: Int? = nil
    var inbox: CommandInboxLinks? = nil
    @Environment(\.scenePhase) private var scenePhase
    @State private var progress: ProjectPlanProgress?
    @State private var project: ProjectSnapshot?
    @State private var revision: UInt64 = 0
    @State private var authorized = false
    @State private var receipts: [String: MobileCommandReceipt] = [:]
    @State private var checkedAt: Date?
    @State private var connectionError: String?
    @State private var readError: String?
    @State private var error: String?
    @State private var busy = false
    @State private var retry = 0
    @State private var readGeneration = 0
    @State private var work: Task<Void, Never>?
    @State private var selection: TodoConfirmation?
    @State private var clearing: PendingMobileCommand?
    @State private var showAll = false
    @State private var legacy = false
    private struct TodoDetail: Identifiable { let id: Int }
    @State private var todoDetail: TodoDetail?
    private var todoIndices: [Int] {
        plan.draft.todos.indices.filter { index in
            (focusedTodoIndex == nil || focusedTodoIndex == index) && (focusedTodoIndex != nil || showAll || progress?.decisions.first(where: { $0.id == index })?.needsFollowUp != false || pending(index) != nil)
        }
    }
    private var commands: [PendingMobileCommand] {
        model.pendingCommands.filter {
            guard $0.machineID == machine.id, $0.request.workspaceID == plan.workspaceID,
                  case let .decidePlanTodo(id, _, _, _) = $0.request.command else { return false }
            return id == plan.id
        }
    }
    private var canControl: Bool {
        active && scenePhase == .active && authorized && project?.state == "open" && progress != nil
        && connectionError == nil && readError == nil && !busy
    }

    var body: some View {
        Group {
            if dashboard { dashboardContent }
            else { detailedList }
        }
        .font(.subheadline).scrollContentBackground(.hidden).background(.black)
        .navigationTitle(dashboard ? "指挥台" : "计划与执行").navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if !dashboard {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { retry += 1 } label: { Image(systemName: "arrow.clockwise") }
                        .accessibilityLabel("刷新执行进度").disabled(busy)
                }
            }
        }
        .sheet(item: $todoDetail) { selected in
            NavigationStack {
                ProjectTodoView(model: model, machine: machine, plan: plan, focusedTodoIndex: selected.id)
                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("完成") { todoDetail = nil } } }
            }
        }
        .sheet(item: $selection) { preview in
            TodoConfirmationView(preview: preview, todo: plan.draft.todos[preview.id], machineName: machine.name,
                                 projectName: project?.displayName ?? "项目", enabled: canControl && isCurrent(preview)) {
                work = Task { await submit(preview) }
            }
        }
        .task(id: "\(scenePhase)-\(retry)-\(active)") {
            project = nil; authorized = false; selection = nil; connectionError = nil
            guard scenePhase == .active && active else { work?.cancel(); return }
            do {
                for try await update in model.client.projectUpdates(machine, workspaceID: plan.workspaceID) {
                    try Task.checkCancellation()
                    switch update {
                    case .current(let snapshot):
                        project = snapshot.projects.first; revision = snapshot.grant.revision
                        authorized = snapshot.grant.projects.contains { $0.id == plan.workspaceID && $0.access == "control" }
                        connectionError = nil
                        if let selection, !isCurrent(selection) { self.selection = nil }
                    case .recovering:
                        project = nil; authorized = false; selection = nil; connectionError = "正在重新连接电脑…"
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                project = nil; authorized = false; selection = nil; connectionError = error.localizedDescription
            }
        }
        .task(id: "decisions-\(scenePhase)-\(retry)-\(liveRevision)-\(active)") {
            readGeneration += 1; progress = nil; checkedAt = nil; receipts = [:]; readError = nil
            guard scenePhase == .active && active else { return }
            do {
                // The dashboard already owns the latest-plan stream; reuse its validated progress.
                if dashboard {
                    guard let liveProgress else { throw MobileError.invalidResponse }
                    try await applyProgress(liveProgress)
                    return
                }
                for try await update in model.client.planUpdates(machine, workspaceID: plan.workspaceID, planID: plan.id) {
                    try Task.checkCancellation()
                    readGeneration += 1
                    switch update {
                    case .legacy(let saved):
                        guard let saved else { throw MobileError.invalidResponse }
                        try await applyProgress(saved); legacy = true
                    case .current(let fresh):
                        try await applyProgress(fresh); legacy = false
                    case .empty: throw MobileError.invalidResponse
                    case .recovering:
                        progress = nil; checkedAt = nil; selection = nil; readError = "正在重新连接电脑…"
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                progress = nil; checkedAt = nil; selection = nil; readError = error.localizedDescription
            }
        }
        .confirmationDialog("已在电脑核实这次决定？", isPresented: Binding(get: { clearing != nil }, set: { if !$0 { clearing = nil } }), titleVisibility: .visible) {
            if let clearing {
                Button("清除本机查询记录", role: .destructive) { work = Task { await clearRecord(clearing) } }
            }
        } message: { Text("清除记录不会取消电脑上的任务，也不会更改已保存的决定。结果未知时请先核实，避免重复安排。") }
        .onDisappear { work?.cancel() }
    }

    private var detailedList: some View {
        List {
            if focusedTodoIndex != nil, let message = connectionError ?? readError ?? error { Text(message).font(.caption).foregroundStyle(.orange) }
            if focusedTodoIndex == nil {
            if let inbox {
                Section { inbox.listRowInsets(EdgeInsets()).listRowBackground(Color.clear) }
            }
            Section {
                VStack(alignment: .leading, spacing: 8) {
                    Label(machine.name, systemImage: "laptopcomputer").font(.caption).foregroundStyle(.secondary)
                    Text(plan.direction).fixedSize(horizontal: false, vertical: true)
                        .frame(minHeight: 44, alignment: .leading).contentShape(Rectangle()).textSelection(.enabled)
                }
            } header: { Text("当前方向") }
            Section {
                Text(plan.draft.summary).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                if legacy { Text("电脑版本较早，执行进度更新后请手动刷新。").font(.caption).foregroundStyle(.secondary) }
                if let checkedAt { Text("核验于 \(checkedAt.formatted(date: .omitted, time: .standard))").font(.caption).foregroundStyle(.secondary) }
                if busy { ProgressView("正在提交决定…") }
                if let message = connectionError ?? readError ?? error {
                    Text(message).font(.caption).foregroundStyle(.orange)
                }
            } header: { Text("工作简报") }
            Section {
                Picker("显示事项", selection: $showAll) {
                    Text("待办").tag(false)
                    Text("全部记录").tag(true)
                }.pickerStyle(.segmented).accessibilityIdentifier("todo-list-scope")
            } header: { Text("Agent 待办") } footer: { Text("已完成和未采纳的事项保留在全部记录中。执行完成后的成果仍需你验收。") }
            if todoIndices.isEmpty {
                Section {
                    ContentUnavailableView("这份计划暂无待办", systemImage: "checkmark.circle", description: Text("可在全部记录中核对执行结果，或返回项目调整下一步方向。"))
                    Button("查看全部记录") { showAll = true }.frame(minHeight: 44)
                }
            }
            }
            ForEach(todoIndices, id: \.self) { index in
                let todo = plan.draft.todos[index]
                Section {
                    Text(todo.title).fontWeight(.medium).fixedSize(horizontal: false, vertical: true)
                    HStack(alignment: .top, spacing: 8) {
                        ProviderMark(profile: todo.target.profile, size: 18)
                        Text(todo.target.profile + " · " + todo.target.title).fixedSize(horizontal: false, vertical: true)
                    }.font(.caption).foregroundStyle(.secondary)
                    DisclosureGroup("完整指令") {
                        Text(todo.instruction).textSelection(.enabled).padding(.vertical, 8)
                    }
                    if let decision = progress?.decisions.first(where: { $0.id == index }) {
                        decisionRow(decision, pending: pending(index))
                    } else if pending(index) == nil {
                        let current = project?.cards?.first { $0.id == todo.target.id }
                        let choice = todo.acceptance(for: current)
                        Button {
                            confirm(index, choice: choice)
                        } label: {
                            HStack(alignment: .top) {
                                Image(systemName: "arrow.up.circle")
                                Text("安排给 Agent").fixedSize(horizontal: false, vertical: true)
                            }.frame(minHeight: 32)
                        }
                            .disabled(!canControl || choice == nil).accessibilityIdentifier("todo-accept-\(index)")
                        Button("不采纳这项建议", role: .destructive) { confirm(index, choice: .reject) }
                            .frame(minHeight: 32).disabled(!canControl).accessibilityIdentifier("todo-reject-\(index)")
                        if !canControl || choice == nil {
                            Text(authorized ? "需要连接电脑并核对当前 Agent，才能安排。" : "当前不可控制，请检查连接和项目权限。")
                                .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    if let command = pending(index) { localRecord(command) }
                } header: { Text("下一步 \(index + 1)") }
            }
        }
    }

    private var dashboardContent: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                CommandBriefCard(machineName: machine.name, agentCount: project?.cardCount,
                    title: project?.cards?.contains(where: \.isBusy) == true ? "一切正在推进" : "下一步，已准备好。",
                    summary: plan.draft.summary).padding(.horizontal, 20).padding(.bottom, 4)
                if let inbox { inbox }
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        NormaMark().frame(width: 16, height: 16)
                        Text("当前方向").font(.caption)
                        Spacer()
                        Button { adjustDirection?() } label: { Text("调整方向").frame(minHeight: 44).contentShape(Rectangle()) }
                            .font(.caption.weight(.semibold)).foregroundStyle(.white)
                            .accessibilityIdentifier("adjust-project-direction").disabled(adjustDirection == nil)
                    }.foregroundStyle(WorkspaceStyle.muted)
                    Text(plan.direction).font(.headline).fixedSize(horizontal: false, vertical: true)
                    Text(todoIndices.first.map { "下一步：" + plan.draft.todos[$0].title } ?? "当前待办已完成，Agent 的结果可在任务中查看。")
                        .font(.caption).foregroundStyle(WorkspaceStyle.muted).fixedSize(horizontal: false, vertical: true)
                }.padding(.horizontal, 22).padding(.top, 12).padding(.bottom, 20)
                if let inbox, let count = inbox.decisionCount, count > 0 {
                    Button(action: inbox.showDecisions) {
                        HStack(spacing: 12) {
                            Image(systemName: "checkmark.shield").font(.title3)
                            VStack(alignment: .leading, spacing: 5) {
                                Text("你现在只需 · \(count) 项待决策").font(.caption2).foregroundStyle(WorkspaceStyle.muted)
                                Text("确认 Agent 的下一步").font(.footnote.weight(.semibold))
                                Text("决定后，由 Agent 继续执行").font(.caption2).foregroundStyle(WorkspaceStyle.muted)
                            }
                            Spacer(minLength: 0)
                            Image(systemName: "chevron.right").font(.caption)
                        }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                            .background(WorkspaceStyle.raised, in: RoundedRectangle(cornerRadius: 22))
                            .overlay(RoundedRectangle(cornerRadius: 22).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                    }.buttonStyle(WorkspacePressStyle()).padding(.horizontal, 20)
                }
                HStack {
                    Text("下一步").font(.headline)
                    Spacer()
                    Button { showAll.toggle() } label: {
                        Text(showAll ? "只看待办" : "全部记录").font(.caption).frame(minHeight: 44).contentShape(Rectangle())
                    }
                        .accessibilityIdentifier("todo-list-scope")
                }.padding(.horizontal, 20).padding(.top, 16)
                Text("你定优先级，Agent 执行并回报结果。").font(.caption2).foregroundStyle(WorkspaceStyle.muted)
                    .padding(.horizontal, 20).padding(.bottom, 8)
                if let message = connectionError ?? readError ?? error {
                    Text(message).font(.caption).foregroundStyle(.orange).padding(.horizontal, 20)
                }
                if todoIndices.isEmpty {
                    WorkspaceStatusView(title: "这份计划暂无待办", icon: "checkmark.circle", message: "完成和未采纳的事项保留在全部记录中。", inline: true) {
                        WorkspaceStatusButton(title: "查看全部记录") { showAll = true }
                    }
                }
                ForEach(todoIndices, id: \.self) { index in
                    Button { todoDetail = TodoDetail(id: index) } label: { dashboardRow(index) }
                        .buttonStyle(WorkspacePressStyle()).accessibilityIdentifier("todo-open-\(index)")
                }
            }.padding(.bottom, 26)
        }.refreshable { retry += 1 }
    }

    private func dashboardRow(_ index: Int) -> some View {
        let todo = plan.draft.todos[index]
        let decision = progress?.decisions.first { $0.id == index }
        return HStack(alignment: .top, spacing: 12) {
            Text(String(format: "%02d", index + 1)).font(.caption.monospaced()).foregroundStyle(WorkspaceStyle.muted).padding(.top, 3)
            VStack(alignment: .leading, spacing: 8) {
                Text(todo.title).font(.footnote.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 5) {
                    ProviderMark(profile: todo.target.profile, size: 14)
                    Text(todo.target.profile + " · " + (project?.displayName ?? "项目")).font(.caption2).foregroundStyle(WorkspaceStyle.muted)
                }
                HStack(spacing: 5) {
                    Circle().frame(width: 5, height: 5)
                    Text(decision.map(todoOutcomeLabel) ?? "等你确认后执行").font(.caption2)
                }.foregroundStyle(WorkspaceStyle.muted).accessibilityIdentifier("todo-summary-\(index)")
            }
            Spacer(minLength: 0)
            Image(systemName: "chevron.right").font(.caption2).foregroundStyle(WorkspaceStyle.muted).padding(.top, 5)
        }.frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 17).padding(.horizontal, 20)
    }

    @ViewBuilder private func decisionRow(_ decision: ProjectTodoDecision, pending: PendingMobileCommand?) -> some View {
        Text(todoOutcomeLabel(decision)).fontWeight(.medium)
            .accessibilityIdentifier("todo-status-\(decision.id)")
        if let project, let route = project.routeID, let card = decision.conversationCard(in: project) {
            NavigationLink("查看所在对话") {
                ConversationView(machine: machine, workspaceID: plan.workspaceID, routeID: route, card: card, model: model)
            }
        } else if decision.deliveryState == "sent" {
            Text("执行会话目前不可访问，请在电脑核实结果。").font(.caption).foregroundStyle(.secondary)
        }
        if decision.retryOf != nil {
            Text("已重新安排，原决定与回执仍保存在电脑上。").font(.caption).foregroundStyle(.secondary)
        }
        if decision.canRetry {
            let todo = plan.draft.todos[decision.id]
            let choice = todo.acceptance(for: project?.cards?.first { $0.id == todo.target.id })
            Button { confirm(decision.id, choice: choice, retryOf: decision.reference) } label: {
                Text("重新安排").fixedSize(horizontal: false, vertical: true).frame(minHeight: 32)
            }.disabled(!canControl || choice == nil || pending != nil)
                .accessibilityIdentifier("todo-retry-\(decision.id)")
        }
        if decision.deliveryState == "queued", let pending,
           decision.receipt.deviceID == pending.deviceID, decision.receipt.operationID == pending.request.operationID {
            Button("撤回待发送任务", role: .destructive) { work = Task { await cancel(pending) } }
                .frame(minHeight: 32).disabled(!canControl).accessibilityIdentifier("todo-cancel-\(decision.id)")
        }
    }
    @ViewBuilder private func localRecord(_ command: PendingMobileCommand) -> some View {
        if let receipt = receipts[command.id], receipt.state == "rejected" {
            Text("这次请求未执行。请核对电脑上的卡片与任务状态。").font(.caption).foregroundStyle(.orange)
            Button("完成查询") { work = Task { await clearRecord(command) } }.disabled(busy)
        } else {
            Text("决定已保存在手机。断线后只查询，不会重复安排。")
                .font(.caption).foregroundStyle(.secondary)
            let replaced = progress?.decisions.contains { decision in
                guard case let .decidePlanTodo(_, index, _, _) = command.request.command,
                      decision.id == index else { return false }
                return decision.receipt.deviceID != command.deviceID || decision.receipt.operationID != command.request.operationID
            } == true
            if replaced { Text("当前安排已更新，这里保留先前的请求供核对。").font(.caption).foregroundStyle(.secondary) }
            if replaced || receipts[command.id] == nil || receipts[command.id]?.state == "outcome_unknown" {
                Button("核实后清除本机记录") { clearing = command }.disabled(busy)
            }
        }
    }
    private func pending(_ index: Int) -> PendingMobileCommand? {
        commands.first { if case let .decidePlanTodo(_, item, _, _) = $0.request.command { item == index } else { false } }
    }
    private func isCurrent(_ preview: TodoConfirmation) -> Bool {
        guard authorized, project?.routeID == preview.route, revision == preview.revision else { return false }
        let previous = progress?.decisions.first { $0.id == preview.id }
        if let retryOf = preview.retryOf {
            guard previous?.canRetry == true, previous?.reference == retryOf else { return false }
        } else if previous != nil { return false }
        if case .reject = preview.choice { return true }
        let todo = plan.draft.todos[preview.id]
        return todo.acceptance(for: project?.cards?.first { $0.id == todo.target.id }) == preview.choice
    }
    private func confirm(_ index: Int, choice: PlanTodoChoice?, retryOf: TodoDecisionRef? = nil) {
        guard canControl, let route = project?.routeID, let choice else { return }
        selection = TodoConfirmation(id: index, route: route, revision: revision, choice: choice, retryOf: retryOf)
    }
    @MainActor private func submit(_ preview: TodoConfirmation) async {
        guard canControl, isCurrent(preview), pending(preview.id) == nil else { selection = nil; return }
        busy = true; error = nil; selection = nil
        defer { busy = false }
        do {
            let command = try await model.client.prepareTodo(machine, routeID: preview.route, revision: preview.revision,
                                                           plan: plan, todoIndex: preview.id, choice: preview.choice, retryOf: preview.retryOf)
            try await model.commandStore.save(command)
            try await model.reloadCommands()
            try Task.checkCancellation()
            receipts[command.id] = try await model.client.submit(command, to: machine)
            try await readProgress()
        } catch {
            guard !Task.isCancelled else { return }
            self.error = error.localizedDescription
            // A lost response never authorizes replay. The stream reconciles the saved decision.
        }
    }
    @MainActor private func applyProgress(_ fresh: ProjectPlanProgress) async throws {
        guard fresh.plan == plan else { throw MobileError.invalidResponse }
        progress = fresh; checkedAt = .now; readError = nil
        if let selection, !isCurrent(selection) { self.selection = nil }
        try await reconcile(fresh, generation: readGeneration)
    }
    @MainActor private func readProgress() async throws {
        let generation = readGeneration
        let fresh = try await model.client.projectPlan(machine, workspaceID: plan.workspaceID, planID: plan.id)
        try Task.checkCancellation()
        guard generation == readGeneration else { return }
        guard fresh.plan == plan else { throw MobileError.invalidResponse }
        // Only the ordered stream updates visible progress. A late one-shot read cannot overwrite it.
        try await reconcile(fresh, generation: generation)
    }
    @MainActor private func reconcile(_ fresh: ProjectPlanProgress, generation: Int) async throws {
        for command in commands {
            try Task.checkCancellation()
            guard generation == readGeneration else { return }
            if let decision = fresh.decisions.first(where: { $0.receipt.operationID == command.request.operationID && $0.receipt.deviceID == command.deviceID }) {
                receipts[command.id] = decision.receipt
                if ["sent", "declined", "cancelled", "not_sent"].contains(decision.deliveryState) { try await clear(command) }
            } else {
                do {
                    let receipt = try await model.client.receipt(for: command, on: machine)
                    try Task.checkCancellation()
                    guard generation == readGeneration else { return }
                    receipts[command.id] = receipt
                } catch MobileError.commandNotFound {
                    error = "电脑上还查不到本机这次决定，请核实后再处理。"
                }
            }
        }
    }
    @MainActor private func cancel(_ command: PendingMobileCommand) async {
        guard canControl else { return }; busy = true; error = nil
        defer { busy = false }
        do { _ = try await model.client.cancelTodoQueue(command, on: machine, revision: revision); try await readProgress() }
        catch { if !Task.isCancelled { self.error = error.localizedDescription } }
    }
    @MainActor private func clearRecord(_ command: PendingMobileCommand) async {
        guard !busy else { return }; busy = true
        defer { busy = false; clearing = nil }
        do { try await clear(command) } catch { self.error = error.localizedDescription }
    }
    @MainActor private func clear(_ command: PendingMobileCommand) async throws {
        try await model.commandStore.remove(id: command.id); try await model.reloadCommands(); receipts.removeValue(forKey: command.id)
    }
}

private func todoOutcomeLabel(_ decision: ProjectTodoDecision) -> String {
    switch decision.outcome?.state {
    case .completed: "本轮执行已完成 · 待验收"
    case .failed: "Agent 执行失败"
    case .interrupted: "任务已停止"
    case nil: todoDeliveryLabel(decision.deliveryState)
    }
}

private func todoDeliveryLabel(_ state: String) -> String {
    switch state {
    case "declined": "未采纳 · 没有安排任务"
    case "dispatching": "正在发送给 Agent"
    case "sent": "Agent 已收到"
    case "queued": "已安排 · 等待 Agent 空闲"
    case "cancelled": "已撤回 · 没有发送"
    case "not_sent": "未发送 · 请核对电脑"
    default: "发送结果未知 · 请核对电脑"
    }
}

struct TodoConfirmation: Identifiable {
    let id: Int
    let route: String
    let revision: UInt64
    let choice: PlanTodoChoice
    let retryOf: TodoDecisionRef?
}

struct TodoConfirmationView: View {
    let preview: TodoConfirmation
    let todo: ProjectPlan.Todo
    let machineName: String
    let projectName: String
    let enabled: Bool
    let onConfirm: () -> Void
    @Environment(\.dismiss) private var dismiss
    private var accepting: Bool { if case .accept = preview.choice { true } else { false } }
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    LabeledContent("电脑", value: machineName)
                    LabeledContent("项目", value: projectName)
                    if preview.retryOf != nil {
                        Text("上次指令未发送。本次重新安排，原决定与回执保留。")
                            .foregroundStyle(.secondary).accessibilityIdentifier("todo-retry-notice")
                    }
                    if case let .accept(target, queue) = preview.choice {
                        HStack(alignment: .top, spacing: 8) {
                            ProviderMark(profile: target.profile, size: 20)
                            Text(target.profile + " · " + target.title).fixedSize(horizontal: false, vertical: true)
                        }
                        Text(queue ? "当前 Agent 正在执行，确认后加入队列。" : "确认后将这条指令发送给 Agent。")
                        if queue, let task = target.taskTitle { Text("当前任务：" + task).foregroundStyle(.secondary) }
                        if target.runID == nil {
                            Text("卡片尚无运行会话，确认后启动新会话。").foregroundStyle(.secondary)
                        } else if target.runID != todo.target.runID || target.sessionID != todo.target.sessionID {
                            Text("Agent 已更换会话。本次将交给当前会话执行。")
                                .foregroundStyle(.orange).accessibilityIdentifier("todo-current-session-notice")
                        }
                    } else { Text("不采纳这项建议，不会向 Agent 发送任务。") }
                } header: { Text(accepting ? "本次执行目标" : "本次决定") }
                Section {
                    Text(todo.title).fontWeight(.medium)
                    Text(todo.instruction).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                } header: { Text("完整指令") }
                Section {
                    Button(accepting ? (preview.retryOf == nil ? "确认安排" : "确认重新安排") : "确认不采纳", role: accepting ? nil : .destructive, action: onConfirm)
                        .frame(minHeight: 32).disabled(!enabled).accessibilityIdentifier("todo-confirm")
                } footer: { Text("决定会保存在电脑上，多台手机共享结果。") }
            }
            .font(.subheadline).scrollContentBackground(.hidden).background(.black)
            .navigationTitle("确认下一步").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("取消") { dismiss() } } }
        }
    }
}

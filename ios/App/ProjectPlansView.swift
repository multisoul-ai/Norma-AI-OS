import SwiftUI
import NormaCore

struct ProjectPlansView: View {
    let model: ComputerModel
    let machine: PairedMachine
    let workspaceID: String
    let name: String
    var initialPlan: ProjectPlan? = nil
    @Environment(\.dismiss) private var dismiss
    @FocusState private var editing: Bool
    @Environment(\.scenePhase) private var scenePhase
    @State private var source: ProjectPlan?
    @State private var latest: ProjectPlan?
    @State private var loadedPlan = false
    @State private var priorityIndex = 0
    @State private var originalRoute: String?
    @State private var retry = 0
    @State private var connectionError: String?
    @State private var planError: String?
    @State private var plans: [ProjectPlan] = []
    @State private var nextCursor: UInt64?
    @State private var project: ProjectSnapshot?
    @State private var revision: UInt64 = 0
    @State private var canGenerate = false
    @State private var direction = ""
    @State private var receipt: MobileCommandReceipt?
    @State private var busy = false
    @State private var error: String?
    @State private var clearing = false
    @State private var work: Task<Void, Never>?
    private var pending: PendingMobileCommand? {
        model.pendingCommands.first { $0.machineID == machine.id && $0.request.workspaceID == workspaceID && $0.request.command.isPlan }
    }

    private var unavailable: String? {
        if let message = connectionError ?? planError { return message }
        if !loadedPlan || project == nil { return "正在核对电脑上的项目…" }
        if !canGenerate { return "需要项目控制权限、可用的电脑连接，以及 Codex 或 Claude 卡片。" }
        if source?.id != latest?.id { return "另一端已更新方向。请载入最新优先事项，再确认你的目标。" }
        if project?.routeID != (source?.routeID ?? originalRoute) { return "项目运行实例已变化，请重新打开调整方向。" }
        if let source, source.draft.todos.indices.contains(priorityIndex) {
            let target = source.draft.todos[priorityIndex].target
            if project?.cards?.contains(where: { $0.id == target.id && $0.profile == target.profile && $0.mobileControl && $0.runID == target.runID && $0.sessionID == target.sessionID }) != true {
                return "所选 Agent 会话已变化，请核对电脑上的卡片后重新梳理。"
            }
        }
        return nil
    }
    private var invalidDirection: Bool { direction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || direction.count > 300 }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    Text("调整方向").font(.title3.weight(.semibold)).accessibilityAddTraits(.isHeader)
                    Spacer(minLength: 8)
                    Button { editing = false; dismiss() } label: {
                        Image(systemName: "xmark").font(.system(size: 14)).frame(width: 44, height: 44)
                            .background(WorkspaceStyle.line, in: Circle())
                    }.accessibilityLabel("关闭").accessibilityIdentifier("direction-close")
                }
                Text("说清目标和取舍，Norma 协调 Agent 推进。")
                    .font(.footnote).foregroundStyle(WorkspaceStyle.muted)
                if let pending { pendingContent(pending) }
                else {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("优先推进").font(.caption).foregroundStyle(WorkspaceStyle.muted)
                        Menu {
                            if let source {
                                ForEach(source.draft.todos.indices, id: \.self) { index in
                                    Button { priorityIndex = index } label: {
                                        Text(source.draft.todos[index].title)
                                        if priorityIndex == index { Image(systemName: "checkmark") }
                                    }.accessibilityIdentifier("plan-priority-\(index)")
                                }
                            }
                        } label: {
                            HStack {
                                Text(priorityTitle).fixedSize(horizontal: false, vertical: true)
                                Spacer(minLength: 8); Image(systemName: "chevron.down").font(.caption)
                            }.padding(16).frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
                                .background(WorkspaceStyle.line.opacity(0.55), in: RoundedRectangle(cornerRadius: 16))
                                .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                        }.accessibilityLabel("优先推进").accessibilityValue(priorityTitle).accessibilityIdentifier("plan-priority")
                            .disabled(busy || source?.draft.todos.isEmpty != false)
                    }
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text("目标与取舍")
                            Spacer(); Text("\(direction.count) / 300").monospacedDigit()
                        }.font(.caption).foregroundStyle(direction.count > 300 ? .orange : WorkspaceStyle.muted)
                        TextField("例如：先验证连接稳定性，界面细节稍后处理。", text: $direction, axis: .vertical)
                            .focused($editing).lineLimit(4...8).font(.footnote).padding(16)
                            .background(WorkspaceStyle.line.opacity(0.55), in: RoundedRectangle(cornerRadius: 16))
                            .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                            .accessibilityLabel("目标与取舍").accessibilityIdentifier("plan-direction").disabled(busy || !loadedPlan)
                    }
                    Text("优先顺序会更新。正在执行的任务保持当前状态，新建议由你确认后交给 Agent，待确认请求仍需单独决定。")
                        .font(.caption).foregroundStyle(WorkspaceStyle.muted).fixedSize(horizontal: false, vertical: true)
                    if let unavailable { Text(unavailable).font(.caption).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true) }
                    if loadedPlan && source?.id != latest?.id {
                        Button("载入最新优先事项，保留已输入目标") { source = latest; priorityIndex = 0 }
                            .font(.footnote).frame(minHeight: 44).accessibilityIdentifier("direction-reload-priority")
                    }
                    Button { editing = false; work = Task { await generate() } } label: {
                        HStack { if busy { ProgressView().tint(.black) }; Text(busy ? "正在梳理…" : "交给 Norma"); Image(systemName: "paperplane") }
                            .font(.footnote.weight(.semibold)).foregroundStyle(.black).frame(maxWidth: .infinity, minHeight: 50)
                            .background(.white, in: Capsule())
                    }.disabled(busy || unavailable != nil || invalidDirection)
                        .opacity(busy || unavailable != nil || invalidDirection ? 0.42 : 1).accessibilityIdentifier("generate-project-plan")
                }
                if let error { Text(error).font(.caption).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true) }
                if connectionError != nil || planError != nil {
                    Button("重新连接") { retry += 1 }.frame(minHeight: 44)
                }
                NavigationLink { history } label: {
                    HStack { Text("方向与计划记录"); Spacer(); Image(systemName: "chevron.right") }
                        .font(.caption).foregroundStyle(WorkspaceStyle.muted).frame(minHeight: 44)
                }.accessibilityIdentifier("direction-history")
            }.padding(20)
        }.scrollDismissesKeyboard(.interactively).background(WorkspaceStyle.raised)
            .buttonStyle(WorkspacePressStyle()).foregroundStyle(.white)
            .toolbar(.hidden, for: .navigationBar)
            .toolbar { ToolbarItemGroup(placement: .keyboard) {
                Spacer(); Button("收起键盘") { editing = false }.accessibilityIdentifier("direction-dismiss-keyboard")
            } }
            .presentationDetents([.large]).presentationDragIndicator(.visible).presentationBackground(WorkspaceStyle.raised)
            .task(id: "project-\(scenePhase)-\(retry)") { await watchProject() }
            .task(id: "plan-\(scenePhase)-\(retry)") { await watchPlan() }
            .onDisappear { work?.cancel() }
            .confirmationDialog("清除这台手机的查询记录？", isPresented: $clearing, titleVisibility: .visible) {
                Button("清除记录", role: .destructive) { if let pending { work = Task { await clear(pending) } } }
            } message: { Text("请先在电脑上核实结果。已保存的计划仍在电脑上，清除记录不会取消生成。") }
    }

    private var priorityTitle: String {
        guard let source, source.draft.todos.indices.contains(priorityIndex) else { return "由 Norma 梳理下一步" }
        return name + " · " + source.draft.todos[priorityIndex].title
    }
    private func pendingContent(_ command: PendingMobileCommand) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            if case let .generatePlan(text, _) = command.request.command { Text(text).font(.footnote).textSelection(.enabled) }
            Text(receiptStatus).font(.caption).foregroundStyle(WorkspaceStyle.muted)
            if busy { WorkspaceLoadingView(message: "正在核对生成结果…", inline: true) }
            Button("查询生成结果") { work = Task { await check(command) } }.frame(minHeight: 44).disabled(busy)
            Button(["succeeded", "rejected"].contains(receipt?.state ?? "") ? "完成查询" : "清除查询记录") {
                if ["succeeded", "rejected"].contains(receipt?.state ?? "") { work = Task { await clear(command) } }
                else { clearing = true }
            }.frame(minHeight: 44).disabled(busy).accessibilityIdentifier("direction-clear-result")
        }
    }
    private var history: some View {
        List {
            if plans.isEmpty { Text("还没有计划建议").foregroundStyle(.secondary) }
            ForEach(plans) { plan in
                NavigationLink { ProjectTodoView(model: model, machine: machine, plan: plan) } label: {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(plan.direction).font(.subheadline.weight(.medium))
                        Text("\(plan.draft.todos.count) 项建议 · \(Date(timeIntervalSince1970: Double(plan.createdAtUnix)).formatted(date: .abbreviated, time: .shortened))")
                            .font(.caption).foregroundStyle(.secondary)
                    }.padding(.vertical, 4)
                }
            }
            if nextCursor != nil { Button("更早的方向与计划") { work = Task { await loadEarlier() } }.disabled(busy) }
        }.scrollContentBackground(.hidden).background(.black).navigationTitle("方向与计划记录")
            .navigationBarTitleDisplayMode(.inline).toolbar(.visible, for: .navigationBar)
            .task {
                do {
                    let page = try await model.client.projectPlans(machine, workspaceID: workspaceID)
                    try Task.checkCancellation(); plans = page.items; nextCursor = page.nextCursor
                } catch { fail(error) }
            }
    }
    @MainActor private func watchProject() async {
        project = nil; canGenerate = false; connectionError = nil
        guard scenePhase == .active else { work?.cancel(); return }
        do {
            for try await update in model.client.projectUpdates(machine, workspaceID: workspaceID) {
                try Task.checkCancellation()
                switch update {
                case .current(let fresh):
                    project = fresh.projects.first; revision = fresh.grant.revision; connectionError = nil
                    if originalRoute == nil { originalRoute = project?.routeID }
                    canGenerate = project?.state == "open" && fresh.grant.projects.contains { $0.id == workspaceID && $0.access == "control" }
                        && project?.cards?.contains { $0.mobileControl && ["Codex", "Claude"].contains($0.profile) } == true
                case .recovering: project = nil; canGenerate = false; connectionError = "正在重新连接电脑…"
                }
            }
        } catch { if !Task.isCancelled { project = nil; canGenerate = false; connectionError = error.localizedDescription } }
    }
    @MainActor private func watchPlan() async {
        planError = nil
        guard scenePhase == .active else { return }
        if !loadedPlan, let initialPlan { source = initialPlan; direction = initialPlan.direction }
        do {
            if let pending { receipt = try await model.client.receipt(for: pending, on: machine) }
            for try await update in model.client.planUpdates(machine, workspaceID: workspaceID) {
                try Task.checkCancellation()
                switch update {
                case .legacy(let progress): acceptLatest(progress?.plan)
                case .current(let progress): acceptLatest(progress.plan)
                case .empty: acceptLatest(nil)
                case .recovering: planError = "正在重新核对当前方向…"
                }
            }
        } catch { if !Task.isCancelled { planError = error.localizedDescription } }
    }
    @MainActor private func acceptLatest(_ plan: ProjectPlan?) {
        latest = plan; planError = nil
        if !loadedPlan { source = plan; direction = plan?.direction ?? direction; loadedPlan = true }
    }

    private var receiptStatus: String {
        switch receipt?.state {
        case "succeeded": "建议已生成并保存在电脑上"
        case "accepted": "Norma 正在梳理，尚未安排执行"
        case "outcome_unknown": "生成结果尚未确认，请查询或在电脑上核实"
        case "rejected":
            switch receipt?.error {
            case "provider_unconfigured": "请先在电脑上配置 Norma 使用的模型"
            case "no_plan_agents": "项目中需要可用的 Codex 或 Claude 卡片"
            case "plan_timeout", "plan_provider_failed": "模型暂未生成建议，可以调整方向后再试"
            case "route_expired", "session_changed": "项目窗口或 Agent 已变化，这次建议未保存"
            case "plan_changed": "另一端已更新方向，这次建议未保存。你的输入仍保留，请核对最新优先事项。"
            case "priority_card_changed": "所选 Agent 会话已变化，这次建议未保存。"
            case "invalid_plan_priority": "模型未保留所选优先事项，这次建议未保存，请调整目标后再试。"
            case "plan_persistence_failed": "电脑未能保存建议，请检查电脑存储状态"
            default: "这次计划未生成，请刷新项目后重新梳理"
            }
        default: "方向已保存在手机上，等待查询结果"
        }
    }
    @MainActor private func generate() async {
        guard !busy, pending == nil, unavailable == nil, !invalidDirection, let route = project?.routeID else { return }
        busy = true; error = nil; receipt = nil
        defer { busy = false }
        do {
            let priority = source.flatMap { plan in
                plan.draft.todos.indices.contains(priorityIndex) ? PlanPriority(planID: plan.id, todoIndex: priorityIndex) : nil
            }
            let command = try await model.client.preparePlan(machine, workspaceID: workspaceID, routeID: route, revision: revision, direction: direction, priority: priority)
            try await model.commandStore.save(command)
            try await model.reloadCommands()
            try Task.checkCancellation()
            receipt = try await model.client.submit(command, to: machine)
            try await poll(command)
        } catch { fail(error) }
    }
    @MainActor private func check(_ command: PendingMobileCommand) async {
        guard !busy else { return }; busy = true; error = nil
        defer { busy = false }
        do {
            receipt = try await model.client.receipt(for: command, on: machine)
            try await poll(command)
        } catch { fail(error) }
    }
    @MainActor private func poll(_ command: PendingMobileCommand) async throws {
        // 30 × 1s matches the Mac command budget; only read receipts, never resubmit.
        for _ in 0..<30 where receipt?.state == "accepted" {
            try await Task.sleep(for: .seconds(1))
            receipt = try await model.client.receipt(for: command, on: machine)
        }
        try Task.checkCancellation()
        if receipt?.state == "succeeded" {
            try await model.commandStore.remove(id: command.id)
            try await model.reloadCommands()
            dismiss()
        }
    }
    @MainActor private func clear(_ command: PendingMobileCommand) async {
        guard !busy else { return }; busy = true
        defer { busy = false }
        do { try await model.commandStore.remove(id: command.id); try await model.reloadCommands(); receipt = nil }
        catch { fail(error) }
    }
    @MainActor private func loadEarlier() async {
        guard !busy, let cursor = nextCursor else { return }; busy = true; error = nil
        defer { busy = false }
        do {
            let page = try await model.client.projectPlans(machine, workspaceID: workspaceID, before: cursor)
            try Task.checkCancellation()
            plans += page.items; nextCursor = page.nextCursor
        } catch { fail(error) }
    }
    @MainActor private func fail(_ failure: Error) {
        guard !Task.isCancelled else { return }
        error = failure.localizedDescription
        if let mobile = failure as? MobileError, [.unauthorized, .wrongComputer, .invalidCredential].contains(mobile) {
            plans = []; nextCursor = nil; project = nil; canGenerate = false
        }
    }
}

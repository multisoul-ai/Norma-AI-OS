import SwiftUI
import NormaCore

/// Shows the latest saved plan directly, reusing the existing Todo decision and progress flow.
struct CommandPlanView: View {
    let model: ComputerModel
    var active = true
    let decisionCount: Int?
    let showDecisions: () -> Void
    let manageComputers: () -> Void
    @Environment(\.scenePhase) private var scenePhase
    @State private var progress: ProjectPlanProgress?
    @State private var progressRevision = 0
    @State private var loading = false
    @State private var error: String?
    @State private var retry = 0
    @State private var editing: ProjectSnapshot?
    @State private var legacy = false
    @State private var deliveryProject: ProjectSnapshot?
    private var project: ProjectSnapshot? { model.selectedProject }
    private var readID: String {
        "\(model.selectedID ?? "")-\(project?.id ?? "")-\(model.grant?.revision ?? 0)-\(scenePhase)-\(retry)-\(active)-\(editing != nil)"
    }

    var body: some View {
        VStack(spacing: 0) {
            if let machine = model.selected, let project {
                if legacy { Text("电脑版本较早，方向更新后请手动刷新。").font(.caption2).foregroundStyle(.secondary).padding(.horizontal, 20) }
                if let progress {
                    ProjectTodoView(model: model, machine: machine, plan: progress.plan, dashboard: true, active: active && editing == nil,
                                    liveProgress: progress, liveRevision: progressRevision, adjustDirection: { editing = project },
                                    inbox: CommandInboxLinks(model: model, machine: machine, project: project,
                                                             decisionCount: decisionCount, showDecisions: showDecisions,
                                                             showing: !active || editing != nil || deliveryProject != nil, open: { deliveryProject = project }))
                        .id(progress.plan.id)
                } else if loading {
                    WorkspaceLoadingView(message: "正在读取行动计划…")
                } else {
                    ScrollView {
                        VStack(spacing: 12) {
                            CommandBriefCard(machineName: machine.name, agentCount: project.cardCount,
                                title: error == nil ? "从一个方向开始。" : "等待重新连接",
                                summary: "你定方向，Agent 负责执行。")
                                .padding(.horizontal, 20)
                            CommandInboxLinks(model: model, machine: machine, project: project,
                                              decisionCount: decisionCount, showDecisions: showDecisions,
                                              showing: !active || editing != nil || deliveryProject != nil, open: { deliveryProject = project })
                                .id(project.id + (project.routeID ?? "") + String(retry))
                            if let error {
                                WorkspaceStatusView(title: "暂时无法读取计划", icon: "wifi.exclamationmark", message: error, inline: true) {
                                    WorkspaceStatusButton(title: "重新读取") { retry += 1 }
                                }
                            } else {
                                WorkspaceStatusView(title: "先定一个方向", icon: "sparkles", message: "告诉 Norma 下一步想推进什么，它会根据项目梳理建议和 Agent 待办。", inline: true) {
                                    WorkspaceStatusButton(title: "梳理下一步") { editing = project }
                                }
                            }
                        }
                    }
                }
            } else if model.selected == nil {
                WorkspaceStatusView(title: "先连接你的电脑", icon: "laptopcomputer.and.iphone", message: "连接 Norma 后，在这里查看方向与 Agent 待办。") {
                    WorkspaceStatusButton(title: "连接电脑", action: manageComputers)
                }
            } else if model.connecting {
                WorkspaceLoadingView(message: "正在读取项目…")
            } else {
                if model.projects == nil {
                    WorkspaceStatusView(title: "暂时无法读取项目", icon: "wifi.exclamationmark", message: "请保持 Norma 打开，并检查网络连接。") {
                        WorkspaceStatusButton(title: "重新连接", action: model.refresh)
                        WorkspaceStatusButton(title: "切换电脑", primary: false, action: manageComputers)
                    }
                } else {
                    WorkspaceStatusView(title: "还没有可访问的项目", icon: "folder", message: "在电脑的手机连接设置中授权项目，然后重新读取。") {
                        WorkspaceStatusButton(title: "重新读取", action: model.refresh)
                    }
                }
            }
        }
        .background(.black)
        .sheet(item: $editing) { project in
            if let machine = model.selected {
                NavigationStack {
                    ProjectPlansView(model: model, machine: machine, workspaceID: project.id, name: project.displayName, initialPlan: progress?.plan)
                }
            }
        }
        .sheet(item: $deliveryProject) { selected in
            if let machine = model.selected, let route = selected.routeID {
                NavigationStack {
                    TaskDeliveriesView(model: model, machine: machine, workspaceID: selected.id, routeID: route, pendingOnly: true)
                        .toolbar { ToolbarItem(placement: .cancellationAction) {
                            Button("完成") { deliveryProject = nil }.accessibilityIdentifier("delivery-inbox-close")
                        } }
                }
            }
        }
        .task(id: readID) {
            guard active && editing == nil else { return }
            progress = nil; error = nil; loading = false; legacy = false
            guard scenePhase == .active, let machine = model.selected, let project else { return }
            let revision = model.grant?.revision
            loading = true
            defer { if !Task.isCancelled { loading = false } }
            do {
                for try await update in model.client.planUpdates(machine, workspaceID: project.id) {
                    try Task.checkCancellation()
                    guard model.selectedID == machine.id, model.selectedProject?.id == project.id,
                          model.grant?.revision == revision else { return }
                    switch update {
                    case .legacy(let saved):
                        progress = saved; progressRevision += 1; loading = false; error = nil; legacy = true
                    case .current(let fresh):
                        progress = fresh; progressRevision += 1; loading = false; error = nil
                    case .empty:
                        progress = nil; loading = false; error = nil
                    case .recovering:
                        progress = nil; loading = true; error = "正在重新连接电脑…"
                    }
                }
            } catch {
                if !Task.isCancelled { progress = nil; self.error = error.localizedDescription }
            }
        }
    }
}

/// A bounded metadata subscription; opening the inbox suspends this watch so only the visible list reads it.
struct CommandInboxLinks: View {
    let model: ComputerModel
    let machine: PairedMachine
    let project: ProjectSnapshot
    let decisionCount: Int?
    let showDecisions: () -> Void
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var count: UInt64?
    @State private var error: String?
    let showing: Bool
    let open: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            (typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(spacing: 8)) : AnyLayout(HStackLayout(spacing: 10))) {
                Button(action: showDecisions) {
                    link("待决策", count: decisionCount.map(String.init), scope: "全部电脑", icon: "checkmark.shield")
                }.accessibilityLabel("待决策，全部电脑").accessibilityValue(decisionCount.map(String.init) ?? "待核验")
                    .accessibilityIdentifier("command-decision-inbox")
                Button(action: open) {
                    link("待验收", count: count.map(String.init), scope: "当前项目", icon: "checkmark")
                }.disabled(project.state != "open" || project.routeID == nil)
                    .accessibilityLabel("待验收，当前项目").accessibilityValue(count.map(String.init) ?? "待核验")
                    .accessibilityIdentifier("command-delivery-inbox")
            }.buttonStyle(.plain)
            if let error { Text(error).font(.caption2).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true) }
        }.padding(.horizontal, 20).padding(.vertical, 8)
        .task(id: "\(scenePhase)-\(showing)-\(model.grant?.revision ?? 0)-\(project.state)") {
            guard !showing else { return }
            count = nil; error = nil
            guard scenePhase == .active, project.state == "open", let route = project.routeID else { return }
            do {
                for try await update in model.client.deliveryUpdates(machine, scope: ProjectReviewRequest(workspaceID: project.id, routeID: route), pendingOnly: true) {
                    try Task.checkCancellation()
                    switch update {
                    case .current(let page): count = page.pendingCount; error = nil
                    case .legacy: count = nil; error = "更新电脑端后可显示待验收总数；现可查看全部成果。"
                    case .recovering: count = nil; error = "待验收数量正在重新核验…"
                    }
                }
            } catch {
                if !Task.isCancelled { count = nil; self.error = "待验收：" + error.localizedDescription }
            }
        }
    }

    private func link(_ title: String, count: String?, scope: String, icon: String) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.caption).foregroundStyle(WorkspaceStyle.muted)
                Text(count.map { $0.count == 1 ? "0" + $0 : $0 } ?? "—")
                    .font(.system(size: 27, weight: .regular)).monospacedDigit()
            }
            Spacer(minLength: 4)
            Image(systemName: icon).font(.system(size: 23, weight: .light)).foregroundStyle(WorkspaceStyle.muted).accessibilityHidden(true)
        }.padding(.vertical, 14).padding(.horizontal, 17).frame(maxWidth: .infinity, minHeight: 80, alignment: .leading)
            .background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 25))
            .overlay(RoundedRectangle(cornerRadius: 25).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
            .contentShape(RoundedRectangle(cornerRadius: 25))
    }
}

struct CommandBriefCard: View {
    let machineName: String
    let agentCount: Int?
    let title: String
    let summary: String
    @ScaledMetric(relativeTo: .title2) private var headingSize = 21.0
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("与 " + machineName + " 同步").font(.caption2).foregroundStyle(WorkspaceStyle.muted).padding(.bottom, 14)
            Text(title).font(.system(size: headingSize, weight: .semibold)).tracking(-0.6)
                .fixedSize(horizontal: false, vertical: true).accessibilityIdentifier("command-hero-title")
            Text(summary).font(.caption).foregroundStyle(WorkspaceStyle.muted).lineSpacing(4)
                .fixedSize(horizontal: false, vertical: true).padding(.top, 6)
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 6) { chips }
                VStack(alignment: .leading, spacing: 6) { chips }
            }.padding(.top, 26)
        }.frame(maxWidth: .infinity, minHeight: 114, alignment: .leading).padding(.vertical, 23).padding(.horizontal, 20)
            .background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 27))
            .overlay(RoundedRectangle(cornerRadius: 27).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
    }
    @ViewBuilder private var chips: some View {
        if let agentCount { chip("\(agentCount) 位 Agent") }
        chip(machineName)
    }
    private func chip(_ title: String) -> some View {
        Text(title).font(.caption2).padding(.horizontal, 8).padding(.vertical, 5)
            .background(WorkspaceStyle.raised, in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
    }
}

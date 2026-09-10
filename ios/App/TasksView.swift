import SwiftUI
import NormaCore

struct TasksView: View {
    let model: ComputerModel
    let active: Bool
    let manageComputers: () -> Void
    let openCommandCenter: (ProjectSnapshot) -> Void
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var projects: [String: ProjectSnapshot] = [:]
    @State private var issues: [String: String] = [:]
    @State private var generation = 0
    @State private var search = ""
    @State private var projectID: String?
    @State private var filter = TaskListFilter.all
    @State private var composing = false
    @State private var newTaskProjectID: String?
    @State private var newTaskCardID: UInt64?
    private var filtering: Bool { !search.isEmpty || filter != .all || projectID != nil }
    private var catalog: [ProjectSnapshot] { model.projects ?? [] }
    private var projectName: String { catalog.first { $0.id == projectID }?.displayName ?? "所有项目" }
    private var watchID: String {
        "\(active)-\(scenePhase)-\(model.selectedID ?? "")-\(model.grant?.revision ?? 0)-" + catalog.map(\.id).joined(separator: "|")
    }
    private var visibleProjects: [ProjectSnapshot] {
        catalog.compactMap { projects[$0.id] }.filter { projectID == nil || $0.id == projectID }
    }
    private var unreadProjects: [ProjectSnapshot] {
        catalog.filter { (projectID == nil || $0.id == projectID) && issues[$0.id] != nil }
    }
    private func cards(in project: ProjectSnapshot) -> [CardSnapshot] {
        project.cards?.filter { $0.matchesTaskSearch(search, projectName: project.displayName, filter: filter) } ?? []
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                taskHeading
                Group {
                if model.selected == nil {
                    WorkspaceStatusView(title: "先连接你的电脑", icon: "laptopcomputer.and.iphone", message: "Agent 在电脑上执行，你随时查看进度、继续对话。") {
                        WorkspaceStatusButton(title: "连接电脑", action: manageComputers)
                    }
                } else if model.projects == nil && !model.connecting {
                    WorkspaceStatusView(title: "暂时无法读取任务", icon: "wifi.exclamationmark", message: "请检查电脑连接。未能读取不代表任务已结束。") {
                        WorkspaceStatusButton(title: "重新连接", action: model.refresh)
                        WorkspaceStatusButton(title: "切换电脑", primary: false, action: manageComputers)
                    }
                } else if catalog.isEmpty && !model.connecting {
                    WorkspaceStatusView(title: "还没有可访问的项目", icon: "folder", message: "在电脑的手机连接设置中授权项目，然后重新读取。") {
                        WorkspaceStatusButton(title: "重新读取", action: model.refresh)
                    }
                } else {
                    taskList
                }
            }
            }
            .background(.black)
            .modifier(WorkspaceChrome(model: model, manageComputers: manageComputers))
            .navigationTitle("任务").navigationBarTitleDisplayMode(.inline)
            .sheet(isPresented: $composing) {
                if let machine = model.selected {
                    NewTaskView(model: model, machine: machine, projects: catalog.compactMap { projects[$0.id] },
                                projectID: $newTaskProjectID, nodeID: $newTaskCardID) { project in
                        composing = false; openCommandCenter(project)
                    }.id(machine.id)
                }
            }
            .onChange(of: model.selectedID) { _, _ in
                composing = false; newTaskProjectID = nil; newTaskCardID = nil
            }
            .task(id: watchID) { await observe() }
            // Preserve the NavigationLink's source while its conversation is presented.
            // The next observation clears stale state before reconnecting.
            .onDisappear { generation += 1 }
        }
    }

    private var taskHeading: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                WorkspacePageTitle(title: "任务").accessibilityIdentifier("tasks-page-title")
                Spacer()
                Button {
                    if newTaskProjectID == nil { newTaskProjectID = projectID }
                    composing = true
                } label: { WorkspaceCircleIcon(symbol: "plus") }
                    .accessibilityLabel("新建任务").accessibilityIdentifier("new-task")
                    .disabled(projects.values.allSatisfy { $0.routeID == nil })
            }
            Menu {
                Picker("项目", selection: $projectID) {
                    Text("所有项目").tag(String?.none)
                    ForEach(catalog) { Text($0.displayName).tag(Optional($0.id)) }
                }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "square.grid.2x2")
                    Text(projectName).lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 10))
                }.font(.footnote).foregroundStyle(WorkspaceStyle.muted).frame(minHeight: 44)
            }.accessibilityLabel("任务项目").accessibilityValue(projectName)
                .accessibilityIdentifier("task-project-filter").disabled(catalog.isEmpty)
        }.buttonStyle(WorkspacePressStyle()).padding(.horizontal, 20).padding(.bottom, 10)
    }

    private var taskList: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                WorkspaceSearch(placeholder: "搜索任务、项目或 Agent", text: $search, identifier: "task-search")
                HStack(spacing: 4) {
                    ForEach(TaskListFilter.allCases, id: \.self) { choice in
                        Button { filter = choice } label: {
                            Text(choice.rawValue).font(.caption.weight(filter == choice ? .semibold : .regular))
                                .frame(maxWidth: .infinity, minHeight: 44)
                                .foregroundStyle(filter == choice ? .black : WorkspaceStyle.muted)
                                .background(filter == choice ? Color(white: 0.98) : .clear, in: Capsule())
                        }.buttonStyle(WorkspacePressStyle()).accessibilityAddTraits(filter == choice ? .isSelected : [])
                            .accessibilityIdentifier(choice == .all ? "task-filter-all" : "task-filter-" + choice.rawValue)
                    }
                }.padding(4).background(WorkspaceStyle.surface, in: Capsule())
                    .overlay(Capsule().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                    .padding(.top, 1).padding(.bottom, 6)
            ForEach(visibleProjects) { project in
                ForEach(cards(in: project)) { card in
                    NavigationLink {
                        if let machine = model.selected, let route = project.routeID {
                            ConversationView(machine: machine, workspaceID: project.id, routeID: route, card: card, model: model)
                        }
                    } label: { taskRow(card, project: project) }
                        .buttonStyle(WorkspacePressStyle()).accessibilityIdentifier("task-\(project.id)-\(card.id)")
                }
            }
            if model.connecting || catalog.contains(where: { projects[$0.id] == nil && issues[$0.id] == nil }) {
                ProgressView("正在同步任务…").font(.caption)
            } else if !unreadProjects.isEmpty {
                WorkspaceStatusView(title: "部分任务暂时无法读取", icon: "wifi.exclamationmark", message: "已连接项目的任务仍会更新，其他项目恢复连接后自动显示。", inline: true) {}
            } else if visibleProjects.allSatisfy({ cards(in: $0).isEmpty }) {
                WorkspaceStatusView(title: search.isEmpty && filter == .all ? "当前没有任务" : "没有匹配的任务", icon: "checklist", message: "先创建卡片，再向 Agent 发送目标。已完成任务的历史结果可在项目成果中查看。", inline: true) {
                    if filtering {
                        WorkspaceStatusButton(title: "清除筛选", action: clearFilters)
                            .accessibilityIdentifier("clear-task-filters")
                    }
                }
            }
            if filtering && (!unreadProjects.isEmpty || visibleProjects.contains(where: { !cards(in: $0).isEmpty })) {
                Button("清除筛选", action: clearFilters)
                    .accessibilityIdentifier("clear-task-filters")
            }
            ForEach(unreadProjects) { project in
                Text(project.displayName + "：" + (issues[project.id] ?? ""))
                    .font(.caption).foregroundStyle(.orange)
            }
            }.padding(.horizontal, 20).padding(.bottom, 26)
        }.scrollDismissesKeyboard(.interactively)
    }

    private func clearFilters() { search = ""; filter = .all; projectID = nil }

    private func taskRow(_ card: CardSnapshot, project: ProjectSnapshot) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 11) {
                ProviderMark(profile: card.profile, size: 26, working: ["thinking", "running_tool"].contains(card.state)).frame(width: 32, height: 32)
                VStack(alignment: .leading, spacing: 5) {
                    Text(card.taskTitle.flatMap { $0.isEmpty ? nil : $0 } ?? card.title)
                        .font(.footnote.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
                    Text(card.profile + " · " + project.displayName).font(.caption2).foregroundStyle(WorkspaceStyle.muted)
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(WorkspaceStyle.muted)
            }
            Text(taskSummary(card)).font(.caption).foregroundStyle(WorkspaceStyle.muted)
                .lineSpacing(5).lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 3)
                .fixedSize(horizontal: false, vertical: true).accessibilityIdentifier("task-summary-\(card.id)")
            HStack(spacing: 8) {
                HStack(spacing: 5) {
                    Circle().frame(width: 5, height: 5)
                    Text(AgentSnapshotCard.status(card.state))
                }.font(.caption2).foregroundStyle(card.state == "waiting_approval" ? Color.orange : Color(white: 0.8))
                    .padding(.horizontal, 8).padding(.vertical, 5).background(WorkspaceStyle.raised, in: Capsule())
                Spacer(minLength: 0)
                if let timing = card.taskTiming, card.isBusy || timing.completedElapsedMs != nil {
                    TimelineView(.periodic(from: .now, by: 60)) { context in
                        let minutes = timing.elapsedMinutes(at: context.date)
                        let elapsed = minutes == 0 ? "不足 1 分钟" : "\(minutes) 分钟"
                        Label {
                            Text((timing.completedElapsedMs == nil ? "已运行 " : "耗时 ") + elapsed)
                                .accessibilityIdentifier("task-elapsed-\(card.id)")
                        } icon: { Image(systemName: "clock").accessibilityHidden(true) }
                            .font(.caption2).foregroundStyle(WorkspaceStyle.muted).monospacedDigit()
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            if let progress = card.taskProgress {
                VStack(alignment: .leading, spacing: 7) {
                    HStack(spacing: 4) {
                        ForEach(0..<progress.total, id: \.self) { step in
                            Capsule().fill(step < progress.completed ? Color.green : step == progress.completed ? Color(white: 0.57).opacity(0.4) : WorkspaceStyle.raised)
                                .frame(height: 3)
                        }
                    }.accessibilityHidden(true)
                    ViewThatFits(in: .horizontal) {
                        HStack(alignment: .top, spacing: 7) {
                            taskStage(progress, card: card)
                            Spacer(minLength: 0)
                            taskStepCount(progress, card: card).fixedSize()
                        }
                        VStack(alignment: .leading, spacing: 5) {
                            taskStage(progress, card: card)
                            taskStepCount(progress, card: card)
                        }
                    }.font(.caption2).foregroundStyle(WorkspaceStyle.muted)
                }.padding(.top, 1)
            }
        }.padding(17).frame(maxWidth: .infinity, alignment: .leading)
            .background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 24))
            .overlay(RoundedRectangle(cornerRadius: 24).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
    }

    private func taskStage(_ progress: TaskProgress, card: CardSnapshot) -> some View {
        Text(progress.completed == progress.total ? "步骤已完成" : (card.isBusy ? "" : "最后阶段 · ") + (progress.currentStep ?? "等待下一步"))
            .fixedSize(horizontal: false, vertical: true).accessibilityIdentifier("task-stage-\(card.id)")
    }

    private func taskStepCount(_ progress: TaskProgress, card: CardSnapshot) -> some View {
        Text("\(progress.completed) / \(progress.total) 步").monospacedDigit()
            .accessibilityLabel("\(progress.completed) / \(progress.total) 步")
            .accessibilityIdentifier("task-progress-count-\(card.id)")
    }

    private func taskSummary(_ card: CardSnapshot) -> String {
        if let summary = card.taskSummary { return summary }
        return switch card.state {
        case "thinking", "running_tool": "Agent 正在电脑上执行，打开对话查看实时输出。"
        case "waiting_approval": "需要你确认下一步操作，确认后由 Agent 继续执行。"
        case "done": "本轮工作已完成，打开对话查看完整回复与执行结果。"
        case "failed": "本轮执行未完成，查看对话中的错误后调整方向。"
        case "ended", "interrupting": "任务已停止，可在原对话中继续安排。"
        default: "查看原 Agent 对话，核对当前执行进度。"
        }
    }

    @MainActor private func observe() async {
        generation += 1
        let current = generation
        projects = [:]; issues = [:]
        guard active, scenePhase == .active, let machine = model.selected, let revision = model.grant?.revision else { return }
        let catalog = catalog
        // Reuse incremental project streams, with no polling or chat-history reads. Streams end on tab/scene changes.
        // ponytail: one stream per authorized project; aggregate server-side if large catalogs hit connection limits.
        let client = model.client
        await withTaskGroup(of: Void.self) { group in
            for project in catalog {
                group.addTask { await observeProject(project, machine: machine, client: client, revision: revision, generation: current) }
            }
        }
    }

    @MainActor private func observeProject(_ project: ProjectSnapshot, machine: PairedMachine, client: MobileClient, revision: UInt64, generation current: Int) async {
        do {
            for try await update in client.projectUpdates(machine, workspaceID: project.id) {
                guard !Task.isCancelled, current == generation, model.selectedID == machine.id,
                      model.grant?.revision == revision else { return }
                switch update {
                case .current(let fresh):
                    guard fresh.grant.revision == revision else {
                        generation += 1
                        projects = [:]; issues = [:]
                        if !model.connecting { model.refresh() }
                        return
                    }
                    let next = fresh.projects.first
                    // Heartbeats revalidate the connection without invalidating an unchanged task list.
                    if projects[project.id] != next { projects[project.id] = next }
                    let issue = next?.cards == nil ? (next?.state == "closed" ? "项目尚未在电脑上打开" : "暂时无法读取卡片") : nil
                    if issues[project.id] != issue { issues[project.id] = issue }
                case .recovering:
                    projects[project.id] = nil; issues[project.id] = "正在重新连接"
                }
            }
        } catch {
            guard !Task.isCancelled, current == generation, model.selectedID == machine.id,
                  model.grant?.revision == revision else { return }
            projects[project.id] = nil; issues[project.id] = error.localizedDescription
        }
    }
}

import SwiftUI
import NormaCore

struct WorkspaceSettingsView: View {
    @Bindable var library: WorkspaceLibraryModel
    let computers: ComputerModel
    @State var workspace: CollaborationWorkspace
    @Environment(\.dismiss) private var dismiss
    @State private var addingProjects = false
    @State private var addingAgents = false
    @State private var renaming = false
    @State private var name = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    section("关联项目", detail: "工作空间可使用的已有项目", symbol: "doc.text") {
                        ForEach(workspace.projects) { project in
                            HStack(spacing: 12) {
                                Image(systemName: "folder").font(.title2).frame(width: 44, height: 44)
                                    .background(IdeaPalette.softGray, in: RoundedRectangle(cornerRadius: 12))
                                VStack(alignment: .leading, spacing: 5) {
                                    Text(project.name).font(.headline)
                                    Text(project.machineName + " · " + (computers.onlineMachineIDs.contains(project.machineID) ? "最近已连接" : "待连接"))
                                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                }
                                Spacer(minLength: 0)
                                Button("移除", role: .destructive) {
                                    var copy = workspace
                                    copy.projects.removeAll { $0.id == project.id }
                                    copy.agents.removeAll { $0.projectReferenceID == project.id }
                                    save(copy)
                                }.frame(minHeight: 44).accessibilityLabel("移除项目 \(project.name)")
                            }
                        }
                        addButton("添加已有项目", id: "workspace-add-project") { addingProjects = true }
                    }
                    section("参与 Agent", detail: "从关联项目选择已有 Agent", symbol: "person.2.fill") {
                        HStack(spacing: 12) {
                            Image("NormaPortrait").resizable().scaledToFill().frame(width: 44, height: 44).clipShape(Circle())
                            VStack(alignment: .leading, spacing: 4) {
                                Text("Norma").font(.headline)
                                Text("协调者").font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                            }
                            Spacer()
                            Text("负责人").font(.caption).foregroundStyle(IdeaPalette.secondary)
                        }
                        ForEach(workspace.agents) { agent in
                            HStack {
                                Image(systemName: "chevron.left.forwardslash.chevron.right").frame(width: 44, height: 44)
                                    .background(IdeaPalette.softGray, in: Circle())
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(agent.name).font(.headline)
                                    Text(agent.profile).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                }
                                Spacer()
                                Button("移除", role: .destructive) {
                                    var copy = workspace; copy.agents.removeAll { $0.id == agent.id }; save(copy)
                                }.frame(minHeight: 44).accessibilityLabel("移除 Agent \(agent.name)")
                            }
                        }
                        addButton("添加 Agent", id: "workspace-add-agent") { addingAgents = true }
                    }
                    Label("关联保存在这部 iPhone。移除项目时也移除其 Agent 关联，电脑上的项目与历史不会被删除。", systemImage: "info.circle")
                        .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                    Button(workspace.archived ? "重开工作空间" : "归档工作空间") {
                        var copy = workspace; copy.archived.toggle(); save(copy)
                    }.frame(minHeight: 44).accessibilityIdentifier("workspace-toggle-archive")
                }.padding(20)
            }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink)
                .navigationTitle("工作空间设置").navigationBarTitleDisplayMode(.inline)
                .safeAreaInset(edge: .top) {
                    Text(workspace.name).font(.subheadline).foregroundStyle(IdeaPalette.secondary).padding(.bottom, 8)
                }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button { dismiss() } label: { Image(systemName: "chevron.left") }
                            .accessibilityLabel("返回工作空间").accessibilityIdentifier("workspace-settings-back")
                    }
                    ToolbarItem(placement: .topBarTrailing) {
                        Button("改名") { name = workspace.name; renaming = true }.accessibilityIdentifier("workspace-rename")
                    }
                }
                .disabled(library.saving)
                .sheet(isPresented: $addingProjects) {
                    WorkspaceAssociationPicker(computers: computers, workspace: workspace, agents: false, save: saveSelection)
                }
                .sheet(isPresented: $addingAgents) {
                    WorkspaceAssociationPicker(computers: computers, workspace: workspace, agents: true, save: saveSelection)
                }
                .alert("工作空间名称", isPresented: $renaming) {
                    TextField("名称", text: $name)
                    Button("取消", role: .cancel) { }
                    Button("保存") { var copy = workspace; copy.name = name; save(copy) }
                }
                .alert("无法保存", isPresented: Binding(get: { library.error != nil }, set: { if !$0 { library.error = nil } })) {
                    Button("好") { library.error = nil }
                } message: { Text(library.error ?? "") }
        }.tint(IdeaPalette.blue).preferredColorScheme(.light)
    }
    private func save(_ value: CollaborationWorkspace) {
        Task { _ = await saveSelection(value) }
    }
    private func saveSelection(_ value: CollaborationWorkspace) async -> Bool {
        guard await library.save(value) else { return false }
        workspace = value
        return true
    }
    private func section<Content: View>(_ title: String, detail: String, symbol: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            Label(title, systemImage: symbol).font(.title2.bold())
            Text(detail).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
            content()
        }.padding(18).frame(maxWidth: .infinity, alignment: .leading).background(.white, in: RoundedRectangle(cornerRadius: 24))
    }
    private func addButton(_ title: String, id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: "plus").font(.headline).foregroundStyle(IdeaPalette.blue)
                .frame(maxWidth: .infinity, minHeight: 50, alignment: .leading)
                .padding(.horizontal, 15).background(IdeaPalette.blue.opacity(0.08), in: RoundedRectangle(cornerRadius: 14))
        }.accessibilityIdentifier(id)
    }
}

private struct WorkspaceAssociationPicker: View {
    let computers: ComputerModel
    @State var workspace: CollaborationWorkspace
    let agents: Bool
    let save: (CollaborationWorkspace) async -> Bool
    @Environment(\.dismiss) private var dismiss
    @State private var projects: [WorkspaceProjectReference] = []
    @State private var definitions: [WorkspaceAgentReference] = []
    @State private var errors: [String] = []
    @State private var loading = true
    @State private var saving = false
    @State private var saveFailed = false

    var body: some View {
        NavigationStack {
            List {
                if loading { ProgressView("正在向电脑核验…") }
                if !loading && (agents ? definitions.isEmpty : projects.isEmpty) {
                    Text(agents ? "暂无可添加的 Agent，请先关联项目，并在电脑中设置 Agent。" : "暂无可添加的项目，请先从更多模块连接电脑。")
                        .foregroundStyle(IdeaPalette.secondary)
                }
                if agents {
                    ForEach(definitions) { agent in
                        Button {
                            if workspace.agents.contains(where: { $0.id == agent.id }) { workspace.agents.removeAll { $0.id == agent.id } }
                            else { workspace.agents.append(agent) }
                        } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(agent.name)
                                    Text(agent.profile).font(.caption).foregroundStyle(IdeaPalette.secondary)
                                    if let project = workspace.projects.first(where: { $0.id == agent.projectReferenceID }) {
                                        Text("\(project.machineName) · \(project.name)").font(.caption).foregroundStyle(IdeaPalette.secondary)
                                    }
                                }
                                Spacer()
                                Image(systemName: workspace.agents.contains(where: { $0.id == agent.id }) ? "checkmark.circle.fill" : "circle")
                            }.frame(minHeight: 44)
                        }.accessibilityIdentifier("workspace-agent-option-\(agent.id)")
                            .accessibilityValue(workspace.agents.contains(where: { $0.id == agent.id }) ? "已选择" : "未选择")
                    }
                } else {
                    ForEach(computers.machines) { machine in
                        Section(machine.name) {
                            ForEach(projects.filter { $0.machineID == machine.id }) { project in
                                Button {
                                    if workspace.projects.contains(where: { $0.id == project.id }) {
                                        workspace.projects.removeAll { $0.id == project.id }
                                        workspace.agents.removeAll { $0.projectReferenceID == project.id }
                                    } else { workspace.projects.append(project) }
                                } label: {
                                    HStack {
                                        Text(project.name)
                                        Spacer()
                                        Image(systemName: workspace.projects.contains(where: { $0.id == project.id }) ? "checkmark.circle.fill" : "circle")
                                    }.frame(minHeight: 44)
                                }.accessibilityIdentifier("workspace-project-option-\(project.id)")
                                    .accessibilityValue(workspace.projects.contains(where: { $0.id == project.id }) ? "已选择" : "未选择")
                            }
                        }
                    }
                }
                ForEach(errors, id: \.self) { Text($0).font(.subheadline).foregroundStyle(.red) }
                if !errors.isEmpty {
                    Button("重新核验") { Task { await loadOptions() } }.disabled(loading)
                }
                if saveFailed { Text("保存失败，已保留当前选择，请重试。 ").font(.subheadline).foregroundStyle(.red) }
            }
            .disabled(saving)
            .navigationTitle(agents ? "添加 Agent" : "添加已有项目").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("取消") { dismiss() }.disabled(saving) }
                ToolbarItem(placement: .confirmationAction) {
                    Button(saving ? "保存中…" : "完成") {
                        saving = true
                        Task {
                            if await save(workspace) { dismiss() }
                            else { saveFailed = true }
                            saving = false
                        }
                    }.disabled(loading || saving).accessibilityIdentifier("workspace-picker-done")
                }
            }
            .task { await loadOptions() }
        }.presentationDetents([.medium, .large]).presentationDragIndicator(.visible).tint(IdeaPalette.blue)
            .interactiveDismissDisabled(saving)
    }
    private func loadOptions() async {
        guard !saving else { return }
        loading = true; errors = []; projects = []; definitions = []
        defer { loading = false }
        for machine in computers.machines {
            if Task.isCancelled { return }
            if agents {
                for project in workspace.projects where project.machineID == machine.id {
                    do {
                        let result = try await computers.client.snapshot(machine, workspaceID: project.projectID)
                        definitions += result.agentDefinitions.filter(\.enabled).map {
                            WorkspaceAgentReference(machineID: machine.id, projectID: project.projectID,
                                                    definitionID: $0.id, name: $0.name, profile: $0.profile)
                        }
                    } catch { errors.append("\(machine.name) · \(project.name)：暂时无法核验，请连接后重试。") }
                }
            } else {
                do {
                    let result = try await computers.client.snapshot(machine)
                    projects += result.projects.filter { $0.state != "unavailable" }.map {
                        WorkspaceProjectReference(machineID: machine.id, projectID: $0.id, name: $0.displayName, machineName: machine.name)
                    }
                } catch { errors.append("\(machine.name)：暂时无法核验，请连接后重试。") }
            }
        }
    }
}

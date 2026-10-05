import SwiftUI
import NormaCore

struct DesktopWorkspaceSettingsView: View {
    @Bindable var model: WorkspaceChatModel
    let computers: ComputerModel
    let cloud: CloudChatModel
    let workspaceID: String
    @Environment(WorkspaceActivityManager.self) private var activities
    @Environment(\.dismiss) private var dismiss
    @State private var addingProject = false
    @State private var addingAgent = false
    @State private var creatingAgent = false
    @State private var addProjectAfterCreating = false
    @State private var rename = false
    @State private var name = ""
    private var workspace: DesktopChatWorkspace? { model.workspace(workspaceID) }
    private var editable: Bool { model.online && !model.updating }
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    if let workspace {
                        section(title:"锁屏总览",subtitle:"跟踪此 Mac 工作空间，与普通通知独立",icon:"lock.rectangle") {
                            Toggle("锁屏跟踪",isOn:Binding(get:{activities.tracks(machine:model.catalog?.macID ?? "",workspace:workspaceID)},set:{value in
                                guard let machine=model.catalog?.macID else {return}
                                Task {await activities.setTracking(value,machine:machine,workspace:workspaceID)}
                            })).disabled(activities.busy || !activities.enabled || !model.online).accessibilityIdentifier("workspace-activity-toggle")
                            Text("iOS 17.2 起可在后台自动出现；17.0/17.1 需先在前台开启。点击卡片后核验并处理。").font(.footnote).foregroundStyle(IdeaPalette.secondary)
                            if !activities.enabled {Text("请在系统设置中允许 Norma 实时活动。").font(.footnote).foregroundStyle(.secondary)}
                            if let error=activities.error {Text(error).font(.footnote).foregroundStyle(.red)}
                        }
                        section(title: "关联项目", subtitle: "工作空间可使用的已有项目", icon: "doc.text") {
                            ForEach(workspace.projects) { project in
                                HStack(spacing: 12) {
                                    Image(systemName: "folder.fill").font(.title2).foregroundStyle(IdeaPalette.blue).frame(width: 44, height: 44).background(IdeaPalette.softGray, in: RoundedRectangle(cornerRadius: 14))
                                    VStack(alignment: .leading, spacing: 5) {
                                        Text(project.name).font(.headline)
                                        Label("\(computers.selected?.name ?? "Mac") · \(model.online ? "已连接" : "离线")", systemImage: model.online ? "checkmark.circle" : "wifi.slash")
                                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                    }
                                    Spacer(minLength: 4)
                                    Button("移除", role: .destructive) { change(.init(kind: "remove_project", areaID: project.areaID)) }
                                        .foregroundStyle(.red)
                                        .frame(minWidth: 44, minHeight: 44).disabled(!editable).accessibilityIdentifier("workspace-remove-project-\(project.areaID)")
                                }.padding(.vertical, 6)
                            }
                            addButton("添加已有项目") { addingProject = true }.accessibilityIdentifier("workspace-add-project")
                        }
                        section(title: "参与 Agent", subtitle: "成员连接 Mac 上已有的卡片和会话", icon: "person.2.fill") {
                            HStack(spacing: 12) {
                                Image("NormaPortrait").resizable().scaledToFill().frame(width: 44, height: 44).clipShape(Circle())
                                VStack(alignment: .leading, spacing: 5) { Text("Norma").font(.headline); Text("Chief").font(.subheadline).foregroundStyle(IdeaPalette.secondary) }
                                Spacer(); Text("负责人").font(.subheadline).foregroundStyle(IdeaPalette.secondary).padding(10).background(IdeaPalette.softGray, in: Capsule())
                            }.padding(.vertical, 6)
                            ForEach(workspace.agents) { member in
                                HStack(spacing: 12) {
                                    Image(systemName: "chevron.left.forwardslash.chevron.right").font(.title3).frame(width: 44, height: 44).background(IdeaPalette.softGray, in: Circle())
                                    VStack(alignment: .leading, spacing: 5) {
                                        Text(member.agent.name).font(.headline)
                                        Text("\(member.project.name) · \(workspaceChatStatus(member.agent.card.state))")
                                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                    }
                                    Spacer(minLength: 4)
                                    Button("移除", role: .destructive) { change(.init(kind: "remove_agent", areaID: member.project.areaID, nodeID: member.agent.card.nodeID)) }
                                        .foregroundStyle(.red)
                                        .frame(minWidth: 44, minHeight: 44).disabled(!editable).accessibilityIdentifier("workspace-remove-agent-\(member.id)")
                                }.padding(.vertical, 6)
                            }
                            addButton("新建智能体") { creatingAgent = true }.accessibilityIdentifier("workspace-create-agent")
                            addButton("添加已有 Agent") { addingAgent = true }.accessibilityIdentifier("workspace-add-agent")
                        }
                        Label("移除关联会同步到 Mac，已有项目文件与群聊历史会保留。", systemImage: "info.circle")
                            .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                        if !model.online { Label("连接 Mac 后才能修改设置", systemImage: "wifi.slash").font(.subheadline).foregroundStyle(IdeaPalette.secondary) }
                        if let error = model.error { Text(error).font(.footnote).foregroundStyle(.red) }
                    } else { ContentUnavailableView("工作空间已不可用", systemImage: "folder.badge.questionmark", description: Text("请返回列表刷新。原有聊天记录仍保存在手机。")) }
                }.padding(20)
            }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink).buttonStyle(IdeaPressStyle())
                .navigationTitle("工作空间设置").navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("完成") { dismiss() } }
                    ToolbarItem(placement: .topBarTrailing) { Button("重命名") { name = workspace?.name ?? ""; rename = true }.disabled(!editable) }
                }
                .sheet(isPresented: $addingProject) { projectPicker }
                .sheet(isPresented: $creatingAgent, onDismiss: { if addProjectAfterCreating { addProjectAfterCreating = false; addingProject = true } }) { AgentCreationSheet(model: model, workspaceID: workspaceID, addProject: { addProjectAfterCreating = true }) }
                .sheet(isPresented: $addingAgent) { agentPicker }
                .alert("重命名工作空间", isPresented: $rename) {
                    TextField("名称", text: $name)
                    Button("取消", role: .cancel) {}
                    Button("保存") { change(.init(kind: "rename", name: name)) }.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
        }.presentationDragIndicator(.visible).tint(IdeaPalette.blue).preferredColorScheme(.light)
    }
    private func change(_ change: WorkspaceConfigurationChange) {
        Task { _ = await model.configure(workspaceID: workspaceID, change: change, computers: computers, cloud: cloud) }
    }
    private func section<Content: View>(title: String, subtitle: String, icon: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 12) {
                Image(systemName: icon).font(.title3).foregroundStyle(IdeaPalette.blue).frame(width: 36, height: 36).background(IdeaPalette.blue.opacity(0.08), in: Circle())
                VStack(alignment: .leading, spacing: 5) { Text(title).font(.title3.bold()); Text(subtitle).font(.subheadline).foregroundStyle(IdeaPalette.secondary) }
            }
            content()
        }.padding(18).frame(maxWidth: .infinity, alignment: .leading).background(.white, in: RoundedRectangle(cornerRadius: 24))
    }
    private func addButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { Label(title, systemImage: "plus").font(.headline).frame(maxWidth: .infinity, minHeight: 48).foregroundStyle(IdeaPalette.blue).background(IdeaPalette.blue.opacity(0.08), in: RoundedRectangle(cornerRadius: 16)) }.disabled(!editable)
    }
    private var projectPicker: some View {
        NavigationStack {
            List {
                ForEach(model.catalog?.availableProjects ?? []) { project in
                    let joined = workspace?.projects.contains { $0.projectID == project.projectID } == true
                    Button {
                        Task { if await model.configure(workspaceID: workspaceID, change: .init(kind: "add_project", projectID: project.projectID), computers: computers, cloud: cloud) { addingProject = false } }
                    } label: { HStack { Label(project.name, systemImage: "folder"); Spacer(); if joined { Image(systemName: "checkmark") } } }
                        .disabled(joined || !editable).accessibilityIdentifier("workspace-existing-project-\(project.projectID)")
                }
                if model.catalog?.availableProjects.isEmpty != false { Text("先在 Mac 上注册项目，然后刷新工作空间。") }
                if let error = model.error { Text(error).foregroundStyle(.red).font(.footnote) }
            }.navigationTitle("添加已有项目").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { addingProject = false } } }
        }.presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
    }
    private var agentPicker: some View {
        NavigationStack {
            List {
                ForEach(workspace?.projects ?? []) { project in
                    Section(project.name) {
                        ForEach(project.definitions.filter(\.enabled)) { definition in
                            let joined = project.agents.contains { $0.definitionID == definition.id }
                            Button {
                                Task { if await model.configure(workspaceID: workspaceID, change: .init(kind: "add_agent", areaID: project.areaID, definitionID: definition.id), computers: computers, cloud: cloud) { addingAgent = false } }
                            } label: { HStack { Text(definition.name); Spacer(); if joined { Image(systemName: "checkmark") } } }
                                .disabled(joined || !editable)
                        }
                    }
                }
                if workspace?.projects.isEmpty != false { Text("先添加已有项目，再选择该项目的 Agent。") }
                if let error = model.error { Text(error).foregroundStyle(.red).font(.footnote) }
            }.navigationTitle("添加 Agent").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { addingAgent = false } } }
        }.presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
    }
}

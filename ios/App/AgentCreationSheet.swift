import SwiftUI
import NormaCore

struct AgentCreationSheet: View {
    @Bindable var model: WorkspaceChatModel
    let workspaceID: String
    var addProject: (() -> Void)? = nil
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var profile = "Codex"
    @State private var areaID = ""
    @State private var submitting = false
    @State private var error: String?
    @State private var receipt: AgentCreationReceipt?
    @State private var unresolved = false
    @State private var checkingPending = false
    private var workspace: DesktopChatWorkspace? { model.workspace(workspaceID) }
    private var projects: [DesktopChatProject] { workspace?.projects ?? [] }
    var body: some View {
        NavigationStack {
            Form {
                if projects.isEmpty {
                    Section {
                        Label("先添加已有项目", systemImage: "folder.badge.plus")
                        Text("在工作空间设置中关联 Mac 上的项目，再创建智能体。")
                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                        Button("添加已有项目") { addProject?(); dismiss() }
                    }
                } else if let agent = receipt?.result {
                    Section {
                        Label("已创建 \(agent.name)", systemImage: "checkmark.circle.fill").foregroundStyle(IdeaPalette.positiveText)
                        Text("\(projects.first { $0.areaID == agent.areaID }?.name ?? "原项目") · \(workspaceChatStatus(agent.card?.state ?? "preparing"))")
                        Text(memberAvailable(agent) ? "可以在群聊中 @\(agent.name)。" : "智能体已创建，成员仍在同步。请回到群聊等待更新，无需再次创建。")
                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    }.accessibilityIdentifier("agent-create-success")
                } else {
                    Section("Agent 类型") {
                        Picker("类型", selection: $profile) {
                            Text("Codex").tag("Codex"); Text("Claude").tag("Claude"); Text("Cursor").tag("Cursor")
                        }.disabled(submitting || unresolved).pickerStyle(.segmented).accessibilityIdentifier("agent-create-profile")
                    }
                    Section("所属项目") {
                        if projects.count == 1, let project = projects.first {
                            Label(project.name, systemImage: "folder").accessibilityElement(children: .combine).accessibilityIdentifier("agent-create-project")
                        } else {
                            Picker("项目", selection: $areaID) {
                                Text("请选择项目").tag("")
                                ForEach(projects) { project in
                                    Text("\(project.name) · \(project.root)").tag(project.areaID)
                                }
                            }.disabled(submitting || unresolved).accessibilityIdentifier("agent-create-project")
                        }
                    }
                    Section {
                        Text("名称由 Mac 自动分配，创建后不会立即派工。")
                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                        if submitting {
                            Label { Text("正在创建…") } icon: { ProgressView() }.accessibilityIdentifier("agent-create-progress")
                        }
                        if submitting || unresolved {
                            Text("关闭页面不会撤销已经提交的创建。").font(.footnote).foregroundStyle(IdeaPalette.secondary)
                        }
                        if let error { Text(error).font(.subheadline).foregroundStyle(.red).accessibilityIdentifier("agent-create-error") }

                    }
                    .disabled(submitting)
                }
            }
            .scrollContentBackground(.hidden).background(IdeaPalette.background)
            .safeAreaInset(edge: .bottom) {
                if !projects.isEmpty, receipt == nil {
                    Button(action: submit) {
                        Text(unresolved ? "查询创建结果" : "创建智能体")
                            .font(.headline).foregroundStyle(.white)
                            .frame(maxWidth: .infinity, minHeight: 24)
                    }
                        .buttonStyle(.borderedProminent).controlSize(.large)
                        .disabled(submitting || checkingPending || areaID.isEmpty).accessibilityIdentifier("agent-create-submit")
                        .padding(.horizontal, 20).padding(.vertical, 12)
                        .background(IdeaPalette.background)
                }
            }
            .foregroundStyle(IdeaPalette.ink).tint(IdeaPalette.blue)
            .navigationTitle("新建智能体").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 2) {
                        Text("新建智能体").font(.headline)
                        Text(workspace?.name ?? "工作空间").font(.caption).foregroundStyle(IdeaPalette.secondary)
                    }
                }
                ToolbarItem(placement: .cancellationAction) {
                    Button(receipt != nil ? "完成" : (submitting || unresolved ? "关闭" : "取消")) { dismiss() }.accessibilityIdentifier("agent-create-cancel")
                }
            }
            .onAppear {
                if projects.count == 1 { areaID = projects[0].areaID }
                else if let macID = model.catalog?.macID {
                    let saved = UserDefaults.standard.string(forKey: "agent-create-area-\(macID)-\(workspaceID)") ?? ""
                    if projects.contains(where: { $0.areaID == saved }) { areaID = saved }
                }
                if let macID = model.catalog?.macID, let saved = UserDefaults.standard.string(forKey: "agent-create-profile-\(macID)-\(workspaceID)"), MobileAgentProfile.supports(saved) { profile = saved }
            }
            .task(id: areaID) { await restorePending() }
            .animation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 1), value: submitting)
        }
        .presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
        .preferredColorScheme(.light)
    }
    private func memberAvailable(_ agent: CreatedWorkspaceAgent) -> Bool {
        workspace?.agents.contains { member in
            member.agent.definitionID == agent.definitionID && member.project.areaID == agent.areaID &&
            member.agent.card.nodeID == agent.card?.nodeID &&
            !["preparing", "failed", "ended", "disabled"].contains(member.agent.card.state)
        } == true
    }
    private func restorePending() async {
        guard let macID = model.catalog?.macID, !areaID.isEmpty else { return }
        let selectedArea = areaID
        checkingPending = true
        defer { checkingPending = false }
        do {
            let pending = try await model.store.pendingCreation(macID: macID, workspaceID: workspaceID, areaID: selectedArea)
            guard !Task.isCancelled, areaID == selectedArea else { return }
            unresolved = pending != nil
            if let pending { profile = pending.request.profile }
        } catch { self.error = error.localizedDescription }
    }
    private func submit() {
        guard !submitting, let macID = model.catalog?.macID else { return }
        submitting = true; error = nil
        UserDefaults.standard.set(areaID, forKey: "agent-create-area-\(macID)-\(workspaceID)")
        UserDefaults.standard.set(profile, forKey: "agent-create-profile-\(macID)-\(workspaceID)")
        Task {
            defer { submitting = false }
            do {
                receipt = try await AgentCreationEnvironment.shared.create(macID: macID, workspaceID: workspaceID, areaID: areaID, profile: profile)
                await model.reloadCreationCatalog(macID: macID)
            } catch {
                await restorePending()
                self.error = error.localizedDescription
            }
        }
    }
}

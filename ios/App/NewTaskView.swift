import SwiftUI
import NormaCore

/// A task targets an existing desktop card. Selecting an Agent never starts another executor.
struct NewTaskView: View {
    let model: ComputerModel
    let machine: PairedMachine
    let projects: [ProjectSnapshot]
    @Binding var projectID: String?
    @Binding var nodeID: UInt64?
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var destination: ProjectSnapshot?
    @State private var destinationCard: CardSnapshot?
    @State private var showConversation = false
    @State private var showDrafts = false
    private var project: ProjectSnapshot? { projects.first { $0.id == projectID } }
    private var cards: [CardSnapshot] { project?.cards?.filter { MobileAgentProfile.supports($0.profile) } ?? [] }
    private var card: CardSnapshot? { cards.first { $0.id == nodeID } }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    HStack {
                        Text("交给 Norma 一件事").font(.title3.weight(.semibold))
                            .fixedSize(horizontal: false, vertical: true).accessibilityAddTraits(.isHeader)
                        Spacer(minLength: 8)
                        Button { dismiss() } label: {
                            Image(systemName: "xmark").font(.system(size: 14))
                                .frame(width: 44, height: 44).background(WorkspaceStyle.raised, in: Circle())
                                .overlay(Circle().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                        }.buttonStyle(WorkspacePressStyle()).accessibilityLabel("关闭")
                            .accessibilityIdentifier("new-task-close")
                    }
                    Text("选择项目，说清目标。任务会交给 Mac 上的 Agent。")
                        .font(.footnote).foregroundStyle(WorkspaceStyle.muted)
                        .fixedSize(horizontal: false, vertical: true)
                    projectPicker
                    agentPicker
                    composer
                }.padding(20)
            }.scrollDismissesKeyboard(.interactively).background(.black)
                .toolbar(.hidden, for: .navigationBar)
                .navigationDestination(isPresented: $showConversation) {
                    if let destination, let route = destination.routeID, let destinationCard {
                        ConversationView(machine: machine, workspaceID: destination.id, routeID: route, card: destinationCard, model: model)
                    }
                }
                .sheet(isPresented: $showDrafts) { DraftHistoryView(book: model.drafts, drafts: model.drafts.drafts) }
                .task(id: projects.map(\.id).joined(separator: "|")) { selectInitialCard() }
        }.presentationDetents(typeSize.isAccessibilitySize || showConversation ? [.large] : [.height(640), .large])
            .presentationDragIndicator(.visible).presentationBackground(.black)
    }

    private var projectPicker: some View {
        VStack(alignment: .leading, spacing: 8) {
            fieldLabel("项目")
            Menu {
                ForEach(projects) { item in
                    Button {
                        if projectID != item.id { projectID = item.id; nodeID = nil; selectInitialCard() }
                    } label: {
                        Label(item.displayName, systemImage: item.id == projectID ? "checkmark" : "folder")
                    }.accessibilityIdentifier("new-task-project-" + item.id)
                }
            } label: {
                selectionLabel(project?.displayName ?? "选择项目", symbol: "folder")
            }.accessibilityLabel("项目").accessibilityValue(project?.displayName ?? "未选择")
                .accessibilityIdentifier("new-task-project").disabled(projects.isEmpty)
        }
    }

    private var agentPicker: some View {
        VStack(alignment: .leading, spacing: 8) {
            fieldLabel("执行 Agent")
            Menu {
                ForEach(MobileAgentProfile.supported, id: \.self) { profile in
                    Section(profile) {
                        ForEach(cards.filter { $0.profile == profile }) { item in
                            Button { nodeID = item.id } label: {
                                Label(item.title, systemImage: item.id == nodeID ? "checkmark" : "bubble.left")
                            }.accessibilityIdentifier("new-task-agent-\(item.id)")
                        }
                    }
                }
            } label: {
                HStack(spacing: 10) {
                    if let card { ProviderMark(profile: card.profile, size: 23, working: false) }
                    VStack(alignment: .leading, spacing: 3) {
                        Text(card?.profile ?? "选择 Agent").foregroundStyle(.white)
                        if let card {
                            Text(card.title + " · " + AgentSnapshotCard.status(card.state))
                                .font(.caption).foregroundStyle(WorkspaceStyle.muted)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.down").font(.caption)
                }.font(.footnote).padding(14).frame(maxWidth: .infinity, minHeight: 52, alignment: .leading)
                    .background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
            }.accessibilityLabel("执行 Agent").accessibilityValue(card.map { $0.profile + " · " + $0.title } ?? "未选择")
                .accessibilityIdentifier("new-task-agent").disabled(cards.isEmpty)
        }
    }

    @ViewBuilder private var composer: some View {
        if let project, let route = project.routeID, let card {
            ChatComposer(model: model, machine: machine, workspaceID: project.id, routeID: route,
                         project: project, card: card, revision: model.grant?.revision ?? 0,
                         canControl: canControl,
                         taskForm: true, saved: { dismiss() }) { _ in
                // Capture the selected original card; no later menu selection can redirect this receipt.
                destination = project; destinationCard = card; showConversation = true
            }.id(ChatDraft.Scope(machineID: machine.id, workspaceID: project.id, routeID: route,
                                 nodeID: card.id, runID: card.runID, sessionID: card.sessionID))
        } else if let project, project.routeID != nil, cards.isEmpty {
            Text("先在项目中创建一张 Agent 卡片，再给它安排任务。")
                .font(.subheadline).foregroundStyle(WorkspaceStyle.muted)
            NavigationLink("打开项目卡片") {
                ProjectCardsView(machine: machine, workspaceID: project.id, name: project.displayName,
                                 model: model)
                    .toolbar(.visible, for: .navigationBar)
            }.font(.footnote).frame(minHeight: 44)
        } else {
            Text("当前项目或卡片暂时不可用。已写入的内容保留在本机草稿中。")
                .font(.subheadline).foregroundStyle(WorkspaceStyle.muted)
            Button("查看本机草稿") { showDrafts = true }
                .font(.footnote).frame(minHeight: 44)
        }
    }

    private var canControl: Bool {
        scenePhase == .active && model.selectedID == machine.id
            && model.grant?.projects.contains(where: { $0.workspaceID == projectID && $0.access == "control" }) == true
    }

    private func selectInitialCard() {
        if projectID == nil { projectID = projects.first?.id }
        if nodeID == nil { nodeID = cards.first?.id }
    }

    private func fieldLabel(_ title: String) -> some View {
        Text(title).font(.caption).foregroundStyle(WorkspaceStyle.muted)
    }

    private func selectionLabel(_ title: String, symbol: String) -> some View {
        HStack(spacing: 10) {
            Image(systemName: symbol).foregroundStyle(WorkspaceStyle.muted)
            Text(title).fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
            Image(systemName: "chevron.down").font(.caption)
        }.font(.footnote).foregroundStyle(.white).padding(14).frame(maxWidth: .infinity, minHeight: 52, alignment: .leading)
            .background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 16))
            .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
    }
}

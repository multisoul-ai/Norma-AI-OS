import SwiftUI
import NormaCore

struct NotificationDestinationView: View {
    let target: PushNotificationTarget
    let model: ComputerModel
    let call: VoiceCall
    let cloudChat: CloudChatModel
    let close: () -> Void
    @Environment(\.scenePhase) private var phase
    @State private var project: ProjectSnapshot?
    @State private var card: CardSnapshot?
    @State private var approval: ApprovalSnapshot?
    @State private var error: String?
    @State private var refresh = 0
    private var machine: PairedMachine? { model.machines.first { $0.id == target.machineID } }

    var body: some View {
        NavigationStack {
            Group {
                if target.kind != .approval, let machine, let project, let card {
                    ConversationView(machine: machine, workspaceID: project.id, routeID: target.routeID, card: card, model: model)
                } else {
                    List {
                        if let machine, let project, let card {
                            Section {
                                Label(machine.name, systemImage: "laptopcomputer").font(.subheadline)
                                Text(project.displayName).font(.caption).foregroundStyle(.secondary)
                                NavigationLink("打开任务对话") {
                                    ConversationView(machine: machine, workspaceID: project.id, routeID: target.routeID, card: card, model: model)
                                }.frame(minHeight: 44)
                            }
                            if let approval {
                                Section("电脑上当前的请求") {
                                    ApprovalContext(snapshot: approval)
                                    NavigationLink("前往指挥台处理") {
                                        CommandCenterView(model: model, call: call, cloudChat: cloudChat, manageComputers: close, focus: target)
                                    }.frame(minHeight: 44)
                                }
                            }
                        } else if let error {
                            ContentUnavailableView("暂时无法打开这项提醒", systemImage: "bell.slash", description: Text(error))
                            Button("重新核验") { refresh += 1 }.frame(minHeight: 44)
                        }
                    }
                    .scrollContentBackground(.hidden).background(.black)
                    .navigationTitle("任务提醒").navigationBarTitleDisplayMode(.inline)
                }
            }
            .workspaceLoading((project == nil || card == nil) && error == nil, "正在向电脑核验任务…")
            .toolbar { ToolbarItem(placement: .cancellationAction) {
                Button("关闭", action: close).accessibilityIdentifier("notification-destination-close")
            } }
            .task(id: "\(phase)-\(refresh)-\(target.id)-\(model.loaded)") {
                project = nil; card = nil; approval = nil; error = nil
                guard phase == .active, model.loaded else { return }
                do {
                    guard let machine else { throw MobileError.wrongComputer }
                    let snapshot = try await model.client.snapshot(machine, workspaceID: target.workspaceID)
                    guard let current = snapshot.projects.first else { throw MobileError.invalidResponse }
                    let live = try target.card(in: current)
                    var request: ApprovalSnapshot?
                    if target.kind == .approval {
                        let items = try await model.client.approvals(machine, request: ApprovalListRequest(workspaceID: target.workspaceID, routeID: target.routeID))
                        guard let found = items.first(where: target.matches) else { throw MobileError.approvalChanged }
                        request = found
                    }
                    let grant = try await model.client.authenticate(machine)
                    guard grant.revision == snapshot.grant.revision else { throw MobileError.unauthorized }
                    try Task.checkCancellation()
                    guard model.machines.contains(where: { $0.id == machine.id }) else { throw MobileError.wrongComputer }
                    project = current; card = live; approval = request
                } catch { if !Task.isCancelled { self.error = error.localizedDescription } }
            }
        }.preferredColorScheme(.dark).tint(.white)
    }
}

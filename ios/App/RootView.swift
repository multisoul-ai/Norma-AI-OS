import SwiftUI
import NormaCore

struct RootView: View {
    @Bindable var model: ComputerModel
    @ObservedObject var notifications: PushNotifications
    @ObservedObject var call: VoiceCall
    @State private var tab = 0
    @State private var showingIdeas = true
    @State private var showComputers = false
    @State private var showNotifications = false
    @State private var ideaLibrary = IdeaLibraryModel()
    @State private var cloudChat = CloudChatModel()
    @State private var workspaces = WorkspaceLibraryModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        NavigationStack {
            home
                .safeAreaInset(edge: .bottom, spacing: 0) { WorkspaceTabBar(selection: tabBinding) }
            .background(IdeaPalette.background.ignoresSafeArea())
            .toolbar(.hidden, for: .navigationBar, .tabBar)
        }
        .sheet(isPresented: $showNotifications) { NotificationSettingsView(model: model, notifications: notifications) }
        .fullScreenCover(isPresented: $showComputers) { ComputersView(model: model, notifications: notifications) }
        .environment(\.workspaceTab, tabBinding)
        .environment(\.workspaceNotifications, { showNotifications = true })
        .environment(\.workspaceIdeas, { showingIdeas = true; tab = 0 })
        .environment(ideaLibrary)
        .environment(model)
        .environment(cloudChat)
        .environment(workspaces)
        .task(id: scenePhase) {
            guard scenePhase == .active else { return }
            while !Task.isCancelled {
                if model.loaded {
                    await workspaces.refreshDelivery(machines: model.machines, cloud: cloudChat, vault: model.vault)
                }
                // Bounded message/task batches stay below the relay's 120 frames/minute ceiling.
                do { try await Task.sleep(for: .seconds(5)) } catch { break }
            }
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .background { workspaces.finishDraftWrites() }
        }
        .onOpenURL { url in
            guard let code = CloudInvitationLink.code(from: url) else { return }
            showComputers = false
            showNotifications = false
            tab = 1
            Task { await cloudChat.claim(code: code) }
        }
        .background(NotificationPresenter(model: model, notifications: notifications, call: call, cloudChat: cloudChat).frame(width: 0, height: 0))
        .alert("连接提示", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("好") { model.error = nil }
        } message: { Text(model.error ?? "") }
    }

    private var tabBinding: Binding<Int> { Binding(get: { tab }, set: { navigate($0) }) }

    @ViewBuilder private var home: some View {
        if tab == 0 {
            if showingIdeas {
                IdeaLibraryView(model: ideaLibrary, workspaces: workspaces, navigateToTab: { navigate($0) },
                                openComputers: { showComputers = true },
                                openSettings: { showNotifications = true }, showsTabBar: false)
            } else {
                WorkspaceLibraryView(library: workspaces, computers: model,
                                     openIdeas: { showingIdeas = true },
                                     openComputers: { showComputers = true })
            }
        } else if tab == 1 {
            CommandCenterView(model: model, call: call, cloudChat: cloudChat, manageComputers: { showComputers = true },
                              active: !showComputers && notifications.target == nil)
        } else { inbox }
    }

    private func navigate(_ destination: Int) {
        if destination == 0 { showingIdeas = false }
        tab = destination
    }

    private var inbox: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                Text("收件箱").font(.largeTitle.bold())
                Text("需要你决定的事项，集中在这里。")
                    .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                CommandDecisionInbox(model: model, manageComputers: { showComputers = true }, active: tab == 2)
                Button { showNotifications = true } label: {
                    Label("任务提醒设置", systemImage: "bell").frame(minHeight: 44)
                }
            }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
        }.foregroundStyle(IdeaPalette.ink)
    }
}

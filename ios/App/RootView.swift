import SwiftUI
import NormaCore

struct RootView: View {
    @Bindable var model: ComputerModel
    @ObservedObject var notifications: PushNotifications
    @ObservedObject var call: VoiceCall
    @State private var tab = 0
    @State private var showComputers = false
    @State private var showProjects = false
    @State private var showNotifications = false
    @State private var projectSearch = ""
    @State private var projectState: String?

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
            TabView(selection: $tab) {
                projects
                    .tabItem { Label("项目", systemImage: "square.grid.2x2") }.tag(0)
                CommandCenterView(model: model, call: call, manageComputers: { showComputers = true }, active: tab == 1 && !showComputers && notifications.target == nil, showsCallControls: false)
                    .tabItem { Label("指挥台", systemImage: "sparkle") }.tag(1)
                TasksView(model: model, active: tab == 2 && !showComputers, manageComputers: { showComputers = true })
                    .id(model.selectedID).tabItem { Label("任务", systemImage: "checklist") }.tag(2)
            }
                if tab == 1 {
                    CallControlsView(call: call, machine: model.selected, project: model.selectedProject)
                    WorkspaceTabBar(selection: $tab).padding(.horizontal, 17).padding(.top, 4).padding(.bottom, 8)
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if tab != 1 {
                    WorkspaceTabBar(selection: $tab).padding(.horizontal, 17).padding(.top, 4).padding(.bottom, 8)
                }
            }
            // The call dock takes layout space so TabView cannot scroll content underneath it.
            // Other tabs keep their existing inset and pushed chats keep keyboard avoidance.
            .ignoresSafeArea(.all, edges: .bottom)
            .environment(\.workspaceTab, $tab)
            .environment(\.workspaceNotifications, { showNotifications = true })
            .toolbar(.hidden, for: .tabBar)
        }.id(model.selectedID)
        .sheet(isPresented: $showNotifications) { NotificationSettingsView(model: model, notifications: notifications) }
        .sheet(isPresented: $showProjects) { projectPicker }
        .onChange(of: model.selectedID) { _, _ in showProjects = false; projectSearch = ""; projectState = nil }
        .sheet(isPresented: $showComputers) { ComputersView(model: model, notifications: notifications) }
        .background(NotificationPresenter(model: model, notifications: notifications, call: call).frame(width: 0, height: 0))
        .alert("连接提示", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("好") { model.error = nil }
        } message: { Text(model.error ?? "") }
    }

    private var projects: some View {
        Group {
            if let machine = model.selected, let project = model.selectedProject {
                ProjectCardsView(machine: machine, workspaceID: project.id, name: project.displayName, model: model, active: tab == 0 && !showComputers,
                                 manageComputers: { showComputers = true }, switchProject: { showProjects = true })
                    .id(project.id)
            } else {
                VStack {
                    if model.connecting { WorkspaceLoadingView(message: "正在读取项目…") }
                    else if model.projects?.isEmpty == true {
                        WorkspaceStatusView(title: "还没有可访问的项目", icon: "folder", message: "在电脑的手机连接设置中授权项目，然后重新读取。") {
                            WorkspaceStatusButton(title: "重新读取", action: model.refresh)
                        }
                    } else if model.selected == nil {
                        WorkspaceStatusView(title: "先连接你的电脑", icon: "laptopcomputer.and.iphone", message: "保持电脑上的 Norma 打开，通过局域网或 Tailscale 连接。") {
                            WorkspaceStatusButton(title: "连接电脑") { showComputers = true }
                        }
                    } else {
                        WorkspaceStatusView(title: "暂时无法读取项目", icon: "wifi.exclamationmark", message: "请保持 Norma 打开，并检查网络连接。") {
                            WorkspaceStatusButton(title: "重新连接", action: model.refresh)
                            WorkspaceStatusButton(title: "切换电脑", primary: false) { showComputers = true }
                        }
                    }
                }.frame(maxWidth: .infinity, maxHeight: .infinity).background(.black)
                    .navigationTitle("项目").navigationBarTitleDisplayMode(.inline)
                    .modifier(WorkspaceChrome(model: model, manageComputers: { showComputers = true }))
            }
        }.id(model.selectedID)
    }

    private var projectPicker: some View {
        NavigationStack {
            List {
                if let projects = model.projects {
                    let visibleProjects = projects.filter { $0.matchesSearch(projectSearch, state: projectState) }
                    if projects.isEmpty {
                        WorkspaceStatusView(title: "还没有可访问的项目", icon: "folder", message: "在电脑的手机连接设置中授权项目，然后重新读取。", inline: true) {
                            WorkspaceStatusButton(title: "重新读取", action: model.refresh)
                        }
                    } else if visibleProjects.isEmpty {
                        WorkspaceStatusView(title: "没有匹配的项目", icon: "magnifyingglass", message: "试试其他名称，或清除状态筛选。", inline: true) {
                            WorkspaceStatusButton(title: "清除筛选") { projectSearch = ""; projectState = nil }
                                .accessibilityIdentifier("clear-project-filters")
                        }
                    }
                    ForEach(visibleProjects) { project in
                        Button {
                            model.selectProject(project.id); showProjects = false
                        } label: {
                            HStack(spacing: 14) {
                                Image(systemName: "folder").font(.title3).foregroundStyle(.secondary)
                                VStack(alignment: .leading, spacing: 5) {
                                    Text(project.displayName).font(.subheadline.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
                                    Text(project.state == "open" ? "\(project.cardCount ?? 0) 张卡片" : project.state == "closed" ? "点按加载项目" : "暂时无法读取")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                if project.id == model.selectedProject?.id { Image(systemName: "checkmark").accessibilityLabel("当前项目") }
                                if model.grant?.projects.first(where: { $0.id == project.id })?.access == "read" {
                                    Text("只读").font(.caption2).foregroundStyle(.secondary)
                                }
                            }.padding(.vertical, 6)
                        }
                    }
                    if let date = model.checkedAt {
                        Section { Text("目录更新于 \(date.formatted(date: .omitted, time: .standard))").font(.caption).foregroundStyle(.secondary) }
                    }
                } else if !model.connecting {
                    WorkspaceStatusView(title: model.selected == nil ? "先连接你的电脑" : "暂时无法读取项目", icon: "wifi.exclamationmark", message: "请保持 Norma 打开，并检查网络连接。", inline: true) {
                        WorkspaceStatusButton(title: "返回项目") { showProjects = false }
                    }
                }
            }
            .scrollContentBackground(.hidden).background(.black)
            .workspaceLoading(model.projects == nil && model.connecting, "正在读取项目…")
            .refreshable { await model.refreshAndWait() }
            .navigationTitle("切换项目").navigationBarTitleDisplayMode(.inline)
            .searchable(text: $projectSearch, placement: .navigationBarDrawer(displayMode: .always), prompt: "搜索项目")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("完成") { showProjects = false }.accessibilityIdentifier("project-picker-close")
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Picker("项目状态", selection: $projectState) {
                            Text("全部状态").tag(String?.none)
                            Text("已加载").tag(Optional("open"))
                            Text("待加载").tag(Optional("closed"))
                            Text("无法读取").tag(Optional("unavailable"))
                        }
                        Button("清除筛选") { projectSearch = ""; projectState = nil }
                    } label: {
                        Image(systemName: projectState == nil ? "line.3.horizontal.decrease.circle" : "line.3.horizontal.decrease.circle.fill")
                    }.accessibilityLabel("筛选项目").accessibilityIdentifier("project-filters")
                }
            }
        }.id(model.selectedID)
        .onChange(of: model.selectedID) { _, _ in projectSearch = ""; projectState = nil }
    }

}

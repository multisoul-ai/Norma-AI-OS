import SwiftUI
import NormaCore

struct DesktopWorkspaceLibraryView: View {
    @Bindable var model: WorkspaceChatModel
    let computers: ComputerModel
    let cloud: CloudChatModel
    let openIdeas: () -> Void
    let openComputers: () -> Void
    @Environment(\.workspaceNotifications) private var openSettings
    @State private var drawer = false
    @State private var creating = false
    @State private var archive = false
    @State private var name = ""
    @State private var search = ""
    @State private var selectedID: String?
    @State private var configuringID: String?
    private var visible: [DesktopChatWorkspace] { model.workspaces.filter { search.isEmpty || $0.name.localizedStandardContains(search) } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Button { drawer = true } label: { Image(systemName: "line.3.horizontal").font(.title3).frame(width: 48, height: 48).background(IdeaPalette.softGray, in: Circle()) }
                    .accessibilityLabel("更多模块").accessibilityIdentifier("workspace-more-modules")
                HStack(alignment: .center) {
                    Text("工作空间").font(.largeTitle.bold())
                    Spacer(minLength: 8)
                    Button { name = ""; creating = true } label: {
                        Label("新建", systemImage: "plus").font(.headline).padding(.horizontal, 16).frame(minHeight: 48)
                            .foregroundStyle(.white).background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 14))
                    }.disabled(!model.online).accessibilityIdentifier("create-workspace")
                }
                Text("连接项目与 Agent，把想法变成成果。").font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                HStack { Image(systemName: "magnifyingglass"); TextField("搜索工作空间", text: $search) }
                    .padding(.horizontal, 14).frame(minHeight: 48).background(.white, in: RoundedRectangle(cornerRadius: 18))
                    .accessibilityIdentifier("workspace-search")
                if !model.online {
                    Label(computers.machines.isEmpty ? "扫码连接 Mac，读取你的工作空间" : "Mac 暂时离线，可查看历史和保留输入", systemImage: "wifi.slash")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                }
                if !model.loaded { ProgressView("正在读取工作空间…") }
                else if visible.isEmpty {
                    ContentUnavailableView("还没有工作空间", systemImage: "folder", description: Text(computers.machines.isEmpty ? "先连接电脑，再创建或打开 Mac 上的工作空间。" : "工作空间与 Mac 共用，想法独立保存在想法库。"))
                    if computers.machines.isEmpty { Button("扫码添加电脑", action: openComputers).frame(minHeight: 44).accessibilityIdentifier("workspace-connect-mac") }
                }
                ForEach(visible) { workspace in
                    Button { selectedID = workspace.id } label: { card(workspace) }
                        .accessibilityIdentifier("workspace-card-\(workspace.id)")
                        .overlay(alignment: .bottomTrailing) {
                            Button { configuringID = workspace.id } label: {
                                Image(systemName: "plus").frame(width: 44, height: 44).background(IdeaPalette.softGray, in: Circle())
                            }.accessibilityLabel("配置 \(workspace.name) 的项目与 Agent").accessibilityIdentifier("workspace-configure-\(workspace.id)").padding(18)
                        }
                }
                if !model.unavailableWorkspaces.isEmpty {
                    Text("已不可用的空间 · 只读历史").font(.headline)
                    ForEach(model.unavailableWorkspaces) { space in
                        Button { selectedID = space.id } label: { Label(space.name, systemImage: "clock.arrow.circlepath").frame(minHeight: 44) }
                    }
                }
                if let error = model.error {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(error).font(.footnote).foregroundStyle(IdeaPalette.secondary)
                        Button("重新连接") { Task { await model.synchronize(computers: computers, cloud: cloud, refresh: true) } }
                            .frame(minHeight: 44).disabled(model.synchronizing)
                    }
                }
            }.padding(20)
        }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink).buttonStyle(IdeaPressStyle())
            .refreshable { await model.synchronize(computers: computers, cloud: cloud, refresh: true) }
            .navigationDestination(item: $selectedID) { id in WorkspaceGroupChatView(model: model, computers: computers, cloud: cloud, workspaceID: id) }
            .sheet(isPresented: Binding(get: { configuringID != nil }, set: { if !$0 { configuringID = nil } })) {
                if let id = configuringID { DesktopWorkspaceSettingsView(model: model, computers: computers, cloud: cloud, workspaceID: id) }
            }
            .sheet(isPresented: $drawer) {
                NavigationStack {
                    List {
                        Button { drawer = false; openIdeas() } label: { Label("想法库", systemImage: "lightbulb") }
                        Button { drawer = false; openComputers() } label: { Label("已连接电脑", systemImage: "laptopcomputer") }
                        Button { drawer = false; archive = true } label: { Label("旧版只读历史", systemImage: "clock.arrow.circlepath") }
                        Button { drawer = false; openSettings() } label: { Label("设置", systemImage: "gearshape") }
                    }.navigationTitle("更多模块").navigationBarTitleDisplayMode(.inline)
                        .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { drawer = false } } }
                }.presentationDetents([.medium]).presentationDragIndicator(.visible)
            }
            .sheet(isPresented: $creating) {
                NavigationStack {
                    Form {
                        TextField("工作空间名称", text: $name).accessibilityIdentifier("workspace-name")
                        Text("创建后会同步到 Mac，项目与 Agent 可在设置中添加。").font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                        if let error = model.error { Text(error).font(.footnote).foregroundStyle(.red) }
                    }.navigationTitle("新建工作空间").navigationBarTitleDisplayMode(.inline)
                        .toolbar {
                            ToolbarItem(placement: .cancellationAction) { Button("取消") { creating = false } }
                            ToolbarItem(placement: .confirmationAction) {
                                Button("创建") {
                                    Task { if await model.configure(workspaceID: "", change: .init(kind: "create", name: name), computers: computers, cloud: cloud) { creating = false } }
                                }.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.updating || !model.online)
                                    .accessibilityIdentifier("workspace-create-save")
                            }
                        }
                }.presentationDetents([.medium]).presentationDragIndicator(.visible)
            }
            .sheet(isPresented: $archive) { LegacyWorkspaceHistoryView() }
    }
    private func card(_ workspace: DesktopChatWorkspace) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 12) {
                Image(systemName: "folder.fill").font(.title2).foregroundStyle(IdeaPalette.positiveText)
                    .frame(width: 48, height: 48).background(IdeaPalette.mint.opacity(0.2), in: RoundedRectangle(cornerRadius: 15))
                VStack(alignment: .leading, spacing: 5) {
                    Text(workspace.name).font(.title3.bold())
                    Text("\(workspace.projects.count) 个项目 · \(workspace.agents.count + 1) 位 Agent")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                }
                Spacer(minLength: 4); Image(systemName: "chevron.right").foregroundStyle(IdeaPalette.secondary)
            }
            if workspace.projects.isEmpty { Text("添加已有项目和 Agent，准备开始协作").font(.subheadline).foregroundStyle(IdeaPalette.secondary) }
            else {
                ViewThatFits(in: .horizontal) {
                    HStack { ForEach(workspace.projects) { Text($0.name).font(.caption).padding(.horizontal, 10).padding(.vertical, 7).background(IdeaPalette.softGray, in: Capsule()) } }
                    Text(workspace.projects.map(\.name).joined(separator: " · ")).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                }
            }
            if let recent = model.messages(workspace.id).last {
                Label(recent.message.text, systemImage: "bubble.left").font(.subheadline.weight(.medium)).lineLimit(2)
                    .padding(12).frame(maxWidth: .infinity, alignment: .leading).background(IdeaPalette.blue.opacity(0.06), in: RoundedRectangle(cornerRadius: 14))
            }
            HStack(spacing: 10) {
                Text("参与的 Agent").font(.caption).foregroundStyle(IdeaPalette.secondary)
                Image("NormaPortrait").resizable().scaledToFill().frame(width: 30, height: 30).clipShape(Circle())
                Text(workspace.agents.isEmpty ? "Norma" : "Norma · " + workspace.agents.map { $0.agent.name }.joined(separator: " · ")).font(.caption).lineLimit(2)
                Spacer(minLength: 44)
            }.frame(minHeight: 44)
        }.padding(18).frame(maxWidth: .infinity, alignment: .leading).background(.white, in: RoundedRectangle(cornerRadius: 24))
    }
}

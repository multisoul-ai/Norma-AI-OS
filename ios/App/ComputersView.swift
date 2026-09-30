import SwiftUI
import NormaCore

struct ComputersView: View {
    @Bindable var model: ComputerModel
    @ObservedObject var notifications: PushNotifications
    @Environment(\.dismiss) private var dismiss
    @State private var adding = false
    @State private var forgetting: PairedMachine?
    @State private var showDrafts = false
    @State private var showQueue = false
    @State private var showNotifications = false
    @State private var projectRoute: (machine: PairedMachine, project: KnownProject)?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    Text("项目来自已连接的电脑")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                        .padding(.top, 8)
                    if model.machines.isEmpty {
                        VStack(alignment: .leading, spacing: 13) {
                            Image(systemName: "laptopcomputer.and.iphone")
                                .font(.system(size: 38)).foregroundStyle(IdeaPalette.blue)
                            Text("你的电脑，随手可达").font(.title3.weight(.semibold))
                            Text("输入验证码或扫码，将这台 iPhone 与你的 Mac 安全配对。")
                                .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(24)
                        .background(.white, in: RoundedRectangle(cornerRadius: 23))
                    } else {
                        ForEach(model.machines) { machine in
                            ConnectedComputerCard(
                                machine: machine,
                                online: model.onlineMachineIDs.contains(machine.id),
                                checked: model.checkedMachineIDs.contains(machine.id),
                                select: { model.select(machine.id); dismiss() },
                                openProject: { project in
                                    model.openProject(project.id, on: machine)
                                    projectRoute = (machine, project)
                                },
                                forget: { forgetting = machine }
                            )
                        }
                    }
                    Text("电脑离线时仍可查看上次同步的项目；打开项目需要重新连接。")
                        .font(.caption).foregroundStyle(IdeaPalette.secondary)
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 22)
            }
            .background(IdeaPalette.background)
            .refreshable { await model.refreshAllMachines() }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                Button { adding = true } label: {
                    Label("扫码添加电脑", systemImage: "qrcode.viewfinder")
                        .font(.headline)
                        .frame(maxWidth: .infinity, minHeight: 56)
                        .foregroundStyle(.white)
                        .background(IdeaPalette.blue, in: Capsule())
                }
                .buttonStyle(IdeaPressStyle())
                .accessibilityIdentifier("add-computer")
                .padding(.horizontal, 20).padding(.top, 12).padding(.bottom, 12)
                .background(.ultraThinMaterial)
            }
            .navigationTitle("已连接电脑").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { dismiss() } label: { Image(systemName: "chevron.left").frame(width: 44, height: 44) }
                        .accessibilityLabel("返回").accessibilityIdentifier("computers-close")
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button("任务提醒", systemImage: "bell") { showNotifications = true }
                            .accessibilityIdentifier("notification-settings")
                        Button("消息队列", systemImage: "text.line.first.and.arrowtriangle.forward") { showQueue = true }
                            .accessibilityIdentifier("local-message-queue")
                        Button("本地草稿", systemImage: "note.text") { showDrafts = true }
                            .accessibilityIdentifier("local-drafts")
                    } label: { Image(systemName: "ellipsis") }
                        .accessibilityLabel("更多电脑操作")
                        .accessibilityIdentifier("computer-more-actions")
                }
            }
        }
        .preferredColorScheme(.light)
        .tint(IdeaPalette.ink)
        .task {
            while !Task.isCancelled {
                await model.refreshAllMachines()
                try? await Task.sleep(for: .seconds(30))
            }
        }
        .fullScreenCover(isPresented: Binding(get: { projectRoute != nil }, set: { if !$0 { projectRoute = nil } })) {
            if let route = projectRoute {
                NavigationStack {
                    ProjectCardsView(machine: route.machine, workspaceID: route.project.id,
                                     name: route.project.name, model: model)
                        .toolbar { ToolbarItem(placement: .cancellationAction) {
                            Button("关闭项目") { projectRoute = nil }.accessibilityIdentifier("computer-project-close")
                        } }
                }.preferredColorScheme(.dark).tint(.white)
            }
        }
        .sheet(isPresented: $adding) { PairComputerView(model: model) }
        .sheet(isPresented: $showNotifications) { NotificationSettingsView(model: model, notifications: notifications) }
        .sheet(isPresented: $showQueue) { MessageQueueView(model: model, commands: model.pendingCommands.filter { $0.request.command.isQueuedMessage }) }
        .sheet(isPresented: $showDrafts) { DraftHistoryView(book: model.drafts, drafts: model.drafts.drafts) }
        .alert("忘记这台电脑？", isPresented: Binding(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } })) {
            if let machine = forgetting {
                Button("忘记 \(machine.name)", role: .destructive) {
                    Task { await model.forget(machine, notifications: notifications) }
                }
            }
            Button("取消", role: .cancel) { forgetting = nil }
        } message: {
            Text("从此 iPhone 移除连接和凭据，重新连接需要配对。会尝试关闭提醒；离线时请在 Mac 上关闭提醒或撤销授权。电脑上的项目和 Agent 不受影响。")
        }
    }
}

private struct ConnectedComputerCard: View {
    let machine: PairedMachine
    let online: Bool
    let checked: Bool
    let select: () -> Void
    let openProject: (KnownProject) -> Void
    let forget: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 12) {
                    Button(action: select) {
                        HStack(spacing: 14) {
                            Image(systemName: "laptopcomputer")
                                .font(.system(size: 42, weight: .ultraLight))
                                .foregroundStyle(online ? IdeaPalette.blue : IdeaPalette.faint)
                                .frame(width: 61, height: 54)
                            Text(machine.name).font(.title3.weight(.semibold))
                            Spacer(minLength: 2)
                        }
                        .frame(maxWidth: .infinity, minHeight: 54, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(IdeaPressStyle())
                    .accessibilityIdentifier("computer-select-" + machine.id)
                    status
                    Menu {
                        Button("忘记这台电脑", systemImage: "trash", role: .destructive, action: forget)
                    } label: { Image(systemName: "ellipsis").frame(width: 35, height: 44) }
                        .accessibilityLabel("管理 " + machine.name)
                        .accessibilityIdentifier("computer-actions-" + machine.id)
                }
                HStack(spacing: 4) {
                    if online { Text("刚刚同步") }
                    else if let date = machine.lastSyncedAt {
                        Text("上次同步")
                        Text(date, style: .relative)
                            .environment(\.locale, Locale(identifier: "zh_CN"))
                    } else { Text(checked ? "尚未同步" : "正在检查连接") }
                    Text("· \(machine.knownProjects?.count ?? 0) 个项目")
                    Text(online ? "· Mac 直连" : "· 本机缓存")
                }
                .font(.caption).foregroundStyle(IdeaPalette.secondary)
                .padding(.leading, 75)
            }
            .padding(.horizontal, 15).padding(.vertical, 14)
            if let projects = machine.knownProjects, !projects.isEmpty {
                ForEach(projects) { project in
                    Rectangle().fill(Color(red: 0.90, green: 0.93, blue: 0.96))
                        .frame(height: 1).padding(.horizontal, 18)
                    Button { openProject(project) } label: {
                        HStack(spacing: 14) {
                            Image(systemName: "folder.fill")
                                .font(.system(size: 24))
                                .foregroundStyle(online ? .white : IdeaPalette.faint)
                                .frame(width: 46, height: 46)
                                .background(online ? IdeaPalette.blue : IdeaPalette.softGray,
                                            in: RoundedRectangle(cornerRadius: 12))
                            VStack(alignment: .leading, spacing: 5) {
                                Text(project.name).font(.body.weight(.semibold))
                                Text(online ? "打开这个项目" : "电脑已离线，暂时不可访问")
                                    .font(.caption).foregroundStyle(IdeaPalette.secondary)
                            }
                            Spacer()
                            Image(systemName: "chevron.right").foregroundStyle(IdeaPalette.faint)
                        }
                        .frame(maxWidth: .infinity, minHeight: 63, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(IdeaPressStyle())
                    .disabled(!online)
                    .accessibilityIdentifier("computer-project-" + machine.id + "-" + project.id)
                    .padding(.horizontal, 18).padding(.vertical, 8)
                }
            } else {
                Text(online ? "这台电脑没有已授权项目" : "连接后显示项目")
                    .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 21).padding(.bottom, 18)
            }
        }
        .foregroundStyle(IdeaPalette.ink)
        .background(.white, in: RoundedRectangle(cornerRadius: 23))
        .shadow(color: IdeaPalette.ink.opacity(0.045), radius: 18, y: 7)
    }

    private var status: some View {
        HStack(spacing: 6) {
            Circle().fill(online ? IdeaPalette.green : IdeaPalette.faint).frame(width: 7, height: 7)
            Text(online ? "在线" : checked ? "离线" : "检查中")
                .font(.caption.weight(.medium))
        }
        .foregroundStyle(online ? Color(red: 0, green: 0.51, blue: 0.34) : IdeaPalette.secondary)
        .padding(.horizontal, 10).frame(minHeight: 29)
        .background(online ? IdeaPalette.mint.opacity(0.28) : IdeaPalette.softGray, in: Capsule())
        .accessibilityIdentifier("computer-status-" + machine.id)
    }
}

struct ComputerAccessButton: View {
    let model: ComputerModel
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Label(model.selected?.name ?? "电脑", systemImage: "laptopcomputer")
                .font(.caption).lineLimit(1)
                .frame(minWidth: 44, minHeight: 44).contentShape(Rectangle())
        }.labelStyle(.iconOnly).dynamicTypeSize(...DynamicTypeSize.large)
            .accessibilityLabel("切换或管理电脑").accessibilityValue(model.selected?.name ?? "未连接")
            .accessibilityIdentifier("manage-computers")
    }
}

/// Shared compact context for the three root tabs; full names remain in each control's accessibility label/value.
struct WorkspaceNavigationTitle: View {
    let title: String
    let subtitle: String
    var showsDisclosure = true

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 5) {
                Text(title).font(.subheadline.weight(.semibold)).lineLimit(1).truncationMode(.middle)
                if showsDisclosure { Image(systemName: "chevron.down").font(.caption2).accessibilityHidden(true) }
            }
            Text(subtitle).font(.caption2).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
        }.dynamicTypeSize(...DynamicTypeSize.large).frame(minWidth: 44, minHeight: 44)
    }
}

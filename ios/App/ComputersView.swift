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

    var body: some View {
        NavigationStack {
            List {
                if model.machines.isEmpty {
                    Section {
                        VStack(alignment: .leading, spacing: 16) {
                            Image(systemName: "laptopcomputer.and.iphone").font(.largeTitle).foregroundStyle(.secondary)
                            Text("你的电脑，随手可达").font(.title3.weight(.semibold))
                            Text("输入验证码或扫码，将这台 iPhone 与你的 Mac 安全配对。").font(.subheadline).foregroundStyle(.secondary)
                        }.padding(.vertical, 16)
                    }
                } else {
                    Section("已保存的电脑") {
                        ForEach(model.machines) { machine in
                            HStack(spacing: 4) {
                                Button { model.select(machine.id); dismiss() } label: {
                                    HStack(spacing: 14) {
                                        Image(systemName: "laptopcomputer").font(.title3).frame(width: 32)
                                        VStack(alignment: .leading, spacing: 4) {
                                            Text(machine.name).font(.subheadline.weight(.semibold))
                                            Text(machine.id == model.selectedID ? "当前电脑" : "轻点切换").font(.caption).foregroundStyle(.secondary)
                                        }
                                        Spacer()
                                        if machine.id == model.selectedID {
                                            Image(systemName: "checkmark.circle.fill").accessibilityLabel("已选择")
                                        }
                                    }.padding(.vertical, 7)
                                }
                                .buttonStyle(.plain)
                                Menu {
                                    Button("忘记这台电脑", systemImage: "trash", role: .destructive) { forgetting = machine }
                                } label: {
                                    Image(systemName: "ellipsis").frame(width: 44, height: 44).contentShape(Rectangle())
                                }.buttonStyle(.borderless).tint(.secondary)
                                    .accessibilityLabel("管理 " + machine.name)
                                    .accessibilityIdentifier("computer-actions-" + machine.id)
                            }
                            .swipeActions(allowsFullSwipe: false) {
                                Button("忘记", systemImage: "trash", role: .destructive) { forgetting = machine }.tint(.red)
                            }
                        }
                    }
                    if model.selected != nil {
                        Section("设备授权") {
                            if model.connecting { WorkspaceLoadingView(message: "正在核验电脑…", inline: true) }
                            else if let grant = model.grant, let date = model.checkedAt {
                                LabeledContent("最近核验", value: date.formatted(date: .omitted, time: .standard))
                                LabeledContent("已授权项目", value: "\(grant.projects.count)")
                                Text(model.projects == nil ? "已核验授权，等待读取项目。" : "已读取当前电脑的项目目录。").font(.caption).foregroundStyle(.secondary)
                            } else { Text("尚未核验授权").foregroundStyle(.secondary) }
                            Button("重新核验") { model.refresh() }.disabled(model.connecting)
                        }.font(.subheadline)
                    }
                }
                Section {
                    Button { showNotifications = true } label: { Label("任务提醒", systemImage: "bell").frame(minHeight: 28) }
                        .accessibilityIdentifier("notification-settings")
                    Button { adding = true } label: { Label("添加电脑", systemImage: "qrcode.viewfinder").frame(minHeight: 28) }
                        .accessibilityIdentifier("add-computer")
                } footer: {
                    Text("可添加多台 Mac，通过局域网或 Tailscale 连接。开启手机服务后，关闭窗口仍可使用；退出 Norma 后停止。")
                }
            }
            .scrollContentBackground(.hidden).background(.black)
            .navigationTitle("电脑").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("完成") { dismiss() }.accessibilityIdentifier("computers-close")
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showQueue = true } label: { Image(systemName: "text.line.first.and.arrowtriangle.forward") }
                        .accessibilityLabel("消息队列").accessibilityIdentifier("local-message-queue")
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showDrafts = true } label: { Image(systemName: "note.text") }
                        .accessibilityLabel("本地草稿").accessibilityIdentifier("local-drafts")
                }
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

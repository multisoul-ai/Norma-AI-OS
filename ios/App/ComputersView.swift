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
                            .swipeActions { Button("忘记", role: .destructive) { forgetting = machine } }
                        }
                    }
                    if model.selected != nil {
                        Section("设备授权") {
                            if model.connecting { ProgressView("正在核验电脑…").font(.subheadline) }
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
                    Text("可添加多台 Mac，通过局域网或 Tailscale 连接。电脑上的 Norma 需要保持打开；关闭窗口后 Agent 会停止。")
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
        .confirmationDialog("忘记这台电脑？", isPresented: Binding(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } }), titleVisibility: .visible) {
            if let machine = forgetting {
                Button("忘记 \(machine.name)", role: .destructive) {
                    Task {
                        guard await notifications.update(machine, to: nil, client: model.client) else {
                            model.error = notifications.errors[machine.id] ?? "请等待提醒设置完成后重试。"; return
                        }
                        await model.forget(machine)
                    }
                }
            }
        } message: { Text("先关闭这台电脑的任务提醒，再移除手机凭据。电脑需要在线；如需撤销电脑上的授权，请在该电脑的 Norma 中操作。") }
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

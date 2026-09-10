import SwiftUI
import UserNotifications
import NormaCore

struct NotificationSettingsView: View {
    let model: ComputerModel
    @ObservedObject var notifications: PushNotifications
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var phase

    var body: some View {
        NavigationStack {
            List {
                if model.machines.isEmpty {
                    ContentUnavailableView("连接电脑后开启任务提醒", systemImage: "bell", description: Text("选择哪些电脑可以提醒你，及时处理 Agent 的请求。"))
                } else {
                    Section("系统通知") {
                        if notifications.authorization == .denied {
                            Text("通知已在系统设置中关闭").font(.subheadline)
                            Button("打开 iOS 通知设置") {
                                if let url = URL(string: UIApplication.openNotificationSettingsURLString) { UIApplication.shared.open(url) }
                            }.frame(minHeight: 44)
                        } else if !notifications.systemAllowed {
                            Button("允许系统通知") { Task { await notifications.requestPermission() } }
                                .frame(minHeight: 44).accessibilityIdentifier("enable-notifications")
                        } else if notifications.registering {
                            ProgressView("正在登记系统通知…").font(.subheadline)
                        } else if notifications.registration == nil {
                            Button("登记此 iPhone 的通知") { notifications.register() }.frame(minHeight: 44)
                        } else {
                            Label("iPhone 通知已就绪", systemImage: "checkmark.circle").font(.subheadline)
                        }
                        if let error = notifications.systemError { Text(error).font(.footnote).foregroundStyle(.orange) }
                    }
                    Section {
                        ForEach(model.machines) { machine in
                            VStack(alignment: .leading, spacing: 10) {
                                Label(machine.name, systemImage: "laptopcomputer").font(.subheadline.weight(.semibold))
                                if notifications.busy.contains(machine.id) { ProgressView("正在核验提醒设置…").font(.caption) }
                                else if let state = notifications.subscriptions[machine.id] {
                                    Text(state.registration == nil ? "任务提醒未开启" : "电脑已登记此 iPhone").font(.caption).foregroundStyle(.secondary)
                                    if state.registration != nil {
                                        if let registration = notifications.registration, state.registration != registration {
                                            Button("更新此 iPhone 的通知登记") {
                                                Task { await notifications.update(machine, to: registration, client: model.client) }
                                            }.frame(minHeight: 44)
                                        }
                                        Button("关闭这台电脑的提醒", role: .destructive) {
                                            Task { await notifications.update(machine, to: nil, client: model.client) }
                                        }.frame(minHeight: 44)
                                    } else {
                                        Button("开启这台电脑的提醒") {
                                            guard let registration = notifications.registration else { return }
                                            Task { await notifications.update(machine, to: registration, client: model.client) }
                                        }.frame(minHeight: 44).disabled(notifications.registration == nil || !notifications.systemAllowed)
                                    }
                                }
                                if let error = notifications.errors[machine.id] { Text(error).font(.footnote).foregroundStyle(.orange) }
                                if notifications.subscriptions[machine.id] == nil && !notifications.busy.contains(machine.id) {
                                    Button("重新核验") { Task { await notifications.load([machine], client: model.client) } }.frame(minHeight: 44)
                                }
                            }.padding(.vertical, 4)
                        }
                    } header: { Text("按电脑接收提醒") } footer: {
                        Text("提醒包括待确认、完成和失败。Mac 需要保持 Norma 打开并配置推送服务；未连接时无法更改电脑上的提醒设置。")
                    }
                }
            }
            .scrollContentBackground(.hidden).background(.black)
            .navigationTitle("任务提醒").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) {
                Button("关闭") { dismiss() }.accessibilityIdentifier("notification-settings-close")
            } }
            .task(id: "\(phase)-" + model.machines.map(\.id).joined()) {
                guard phase == .active else { return }
                await notifications.refreshSystem()
                await notifications.load(model.machines, client: model.client)
            }
        }
    }
}

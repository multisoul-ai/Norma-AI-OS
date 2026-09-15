import SwiftUI
import NormaCore

struct CommandCenterView: View {
    @Bindable var model: ComputerModel
    @ObservedObject var call: VoiceCall
    let manageComputers: () -> Void
    var active = true
    var showsCallControls = true
    var focus: PushNotificationTarget? = nil
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var showingPlans = false
    @State private var showingDiscussions = false
    @State private var showingOperations = false
    private var machine: PairedMachine? { call.isCalling ? call.target?.machine : model.selected }
    private var project: ProjectSnapshot? { call.isCalling ? call.target?.project : model.selectedProject }

    var body: some View {
        VStack(spacing: 0) {
            heading
            GeometryReader { geometry in
                ScrollView {
                    VStack(spacing: 0) {
                        if let error = call.avatar.loadError {
                            ContentUnavailableView("角色暂时无法显示", systemImage: "person.crop.circle.badge.exclamationmark", description: Text(error))
                        } else {
                            AvatarView(avatar: call.avatar, degraded: call.degraded,
                                       active: active && scenePhase == .active && !call.showSettings,
                                       onTouch: { head in _ = call.avatar.touch(head: head) })
                                .frame(height: max(220, min(440, geometry.size.height * 0.72)))
                                .clipped()
                                .mask(LinearGradient(stops: [.init(color: .black, location: 0),
                                                             .init(color: .black, location: 0.9),
                                                             .init(color: .clear, location: 1)],
                                                     startPoint: .top, endPoint: .bottom))
                        }
                        VStack(spacing: 12) {
                            Text(call.isCalling ? call.status : "N O R M A")
                                .font(.caption).foregroundStyle(Color(red: 0.72, green: 0.84, blue: 0.78))
                                .accessibilityIdentifier("realtime-call-status")
                            VStack(spacing: 10) {
                                Text(call.subtitles && !call.subtitle.isEmpty ? call.subtitle : (call.isCalling ? "我在这里，随时可以插话。" : "一起，把想法变成现实。"))
                                    .font(.title2.weight(.medium)).multilineTextAlignment(.center)
                                    .fixedSize(horizontal: false, vertical: true).accessibilityIdentifier("realtime-subtitle")
                                if call.subtitles && !call.userSubtitle.isEmpty {
                                    Text(call.userSubtitle).font(.subheadline).foregroundStyle(.secondary).textSelection(.enabled)
                                }
                                Text(call.isCalling ? call.toolsStatusText : call.status)
                                    .font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
                                    .fixedSize(horizontal: false, vertical: true)
                                if let error = call.targetError {
                                    Text(error).font(.caption).foregroundStyle(.orange)
                                        .accessibilityIdentifier("realtime-target-error")
                                }
                                if let failure = call.discussions.failure {
                                    Button { showingDiscussions = true } label: {
                                        Label(failure, systemImage: "exclamationmark.bubble")
                                            .font(.caption).foregroundStyle(.orange).frame(minHeight: 44)
                                    }.accessibilityIdentifier("realtime-discussion-save-error")
                                }
                                if let recovery = call.recoveryStatus {
                                    Button { showingOperations = true } label: {
                                        Label(recovery, systemImage: "clock.badge.exclamationmark")
                                            .font(.caption).foregroundStyle(.orange).frame(minHeight: 44)
                                    }.accessibilityIdentifier("realtime-operation-recovery")
                                }
                            }
                            (typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(spacing: 8)) : AnyLayout(HStackLayout(spacing: 8))) {
                                CommandDecisionInbox(model: model, manageComputers: manageComputers, active: active, focus: focus)
                                if let machine, let project {
                                    CommandDeliveryReminder(model: model, machine: machine, project: project, active: active)
                                        .id(machine.id + project.id)
                                }
                            }.buttonStyle(WorkspacePressStyle())
                        }.padding(.horizontal, 20).padding(.top, 8).padding(.bottom, 16)
                    }
                }.scrollIndicators(.hidden)
            }
            if showsCallControls { CallControlsView(call: call, machine: model.selected, project: model.selectedProject) }
        }.background(.black)
            .navigationTitle("指挥台").navigationBarTitleDisplayMode(.inline)
            .toolbar(.hidden, for: .navigationBar, .tabBar)
            .sheet(isPresented: $call.showSettings) { CallSettingsView(call: call, manageComputers: manageComputers) }
            .sheet(isPresented: $showingDiscussions) { DiscussionDraftsView(book: call.discussions) }
            .sheet(isPresented: $showingOperations) { OperationRecordsView(call: call) }
            .sheet(isPresented: $showingPlans) {
                if let machine, let project {
                    NavigationStack {
                        ProjectPlansView(model: model, machine: machine, workspaceID: project.id, name: project.displayName)
                    }
                }
            }
            .onChange(of: reduceMotion, initial: true) { _, value in
                call.reduceMotion = value
                call.avatar.update(phase: call.phase, level: call.realtime?.audio.snapshot().level ?? 0,
                                   reduceMotion: value, degraded: call.degraded, userSpeaking: call.userSpeaking)
            }
    }

    private var heading: some View {
        HStack(spacing: 12) {
            Menu {
                if call.isCalling {
                    Text("本次通话：" + (project?.displayName ?? "未绑定项目"))
                    if let selected = model.selected {
                        Section("切换到 \(selected.name) 的项目") {
                            ForEach(model.projects ?? []) { item in
                                Button {
                                    Task { await call.switchTarget(machine: selected, workspaceID: item.id) }
                                } label: {
                                    if selected.id == call.target?.machine.id && item.id == project?.id {
                                        Label(item.displayName, systemImage: "checkmark")
                                    } else { Text(item.displayName) }
                                }.disabled(call.selectingTarget || (selected.id == call.target?.machine.id && item.id == project?.id))
                                    .accessibilityIdentifier("realtime-switch-project-\(item.id)")
                            }
                        }
                    }
                } else {
                    ForEach(model.projects ?? []) { item in
                        Button { model.selectProject(item.id) } label: {
                            if item.id == model.selectedProject?.id { Label(item.displayName, systemImage: "checkmark") }
                            else { Text(item.displayName) }
                        }
                    }
                }
                if project != nil {
                    Button("方向与计划记录") { showingPlans = true }.accessibilityIdentifier("command-project-plans")
                }
                Button("讨论草稿") { showingDiscussions = true }.accessibilityIdentifier("realtime-discussions")
                Button("任务操作记录") { showingOperations = true }.accessibilityIdentifier("realtime-operations")
                Button("切换或管理电脑", action: manageComputers)
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: "folder").font(.subheadline).foregroundStyle(.secondary)
                    Text(project?.displayName ?? "未绑定项目").font(.subheadline.weight(.semibold))
                        .lineLimit(2).multilineTextAlignment(.leading)
                    Image(systemName: "chevron.down").font(.caption2)
                }.frame(minHeight: 44, alignment: .leading)
            }.accessibilityLabel("通话项目").accessibilityValue(project?.displayName ?? "未绑定项目")
                .accessibilityIdentifier("realtime-project")
            Spacer(minLength: 0)
            Button { call.showSettings = true } label: {
                Image(systemName: "slider.horizontal.3").font(.body).frame(width: 44, height: 44)
                    .background(WorkspaceStyle.surface, in: Circle())
                    .overlay(Circle().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
            }.accessibilityLabel("通话设置").accessibilityIdentifier("realtime-settings")
        }.buttonStyle(WorkspacePressStyle()).padding(.horizontal, 20).padding(.vertical, 4)
    }

}

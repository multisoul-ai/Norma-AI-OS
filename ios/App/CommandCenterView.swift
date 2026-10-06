import SwiftUI
import NormaCore

struct CommandCenterView: View {
    @Bindable var model: ComputerModel
    @ObservedObject var call: VoiceCall
    @Bindable var cloudChat: CloudChatModel
    let manageComputers: () -> Void
    var active = true
    var showsCallControls = true
    var focus: PushNotificationTarget? = nil
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var typing = false
    @State private var showingPlans = false
    @State private var showingDiscussions = false
    @State private var showingOperations = false
    private var machine: PairedMachine? { call.isCalling ? call.target?.machine : model.selected }
    private var project: ProjectSnapshot? { call.isCalling ? call.target?.project : model.selectedProject }

    var body: some View {
        VStack(spacing: 0) {
            heading
            HStack(spacing: 12) {
                if typing {
                    Image("NormaPortrait").resizable().scaledToFill()
                        .frame(width: 52, height: 52).clipShape(Circle()).accessibilityHidden(true)
                }
                VStack(spacing: 6) {
                    Text("Norma").font(typing ? .title.bold() : .largeTitle.bold())
                    Text(call.isCalling ? call.state.rawValue : cloudChat.sending ? "正在回复" : "随时可以聊聊")
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(call.isCalling ? IdeaPalette.green : IdeaPalette.secondary)
                        .accessibilityIdentifier("realtime-call-status")
                }
            }.padding(.vertical, 12)
            if typing {
                CloudChatView(model: cloudChat)
            } else {
                GeometryReader { geometry in
                    ScrollView {
                        VStack(spacing: 0) {
                            if call.isCalling && call.avatar.loadError == nil {
                                AvatarView(avatar: call.avatar, degraded: call.degraded,
                                           active: active && scenePhase == .active && !call.showSettings,
                                           onTouch: { head in _ = call.avatar.touch(head: head) })
                                    .frame(height: max(200, geometry.size.height * 0.65))
                            } else {
                                Image("NormaCompanion").resizable().scaledToFit()
                                    .frame(width: geometry.size.width, height: max(200, geometry.size.height * 0.72))
                                    .clipped()
                                    .mask(LinearGradient(stops: [.init(color: .black, location: 0),
                                                                 .init(color: .black, location: 0.84),
                                                                 .init(color: .clear, location: 1)],
                                                         startPoint: .top, endPoint: .bottom))
                                    .accessibilityLabel("Norma")
                            }
                            VStack(alignment: .leading, spacing: 10) {
                                Label(call.isCalling ? "实时对话" : "和 Norma 聊聊", systemImage: call.isCalling ? "waveform" : "sparkles")
                                    .font(.subheadline).foregroundStyle(IdeaPalette.blue)
                                Text(replyText).font(.title3.weight(.medium))
                                    .fixedSize(horizontal: false, vertical: true)
                                    .accessibilityIdentifier("realtime-subtitle")
                                if call.isCalling {
                                    Text(call.toolsStatusText).font(.caption).foregroundStyle(IdeaPalette.secondary)
                                } else if cloudChat.enrolling {
                                    ProgressView("正在接受邀请…")
                                }
                                if let error = cloudChat.error {
                                    Text(error).font(.subheadline).foregroundStyle(.red)
                                }
                                if let error = call.targetError {
                                    Text(error).font(.caption).foregroundStyle(.red).accessibilityIdentifier("realtime-target-error")
                                }
                                if call.state == .paused || call.state == .failed {
                                    Text(call.status).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                }
                            }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
                                .background(.white, in: RoundedRectangle(cornerRadius: 24))
                                .padding(.horizontal, 20).padding(.bottom, 12)
                            if focus != nil {
                                CommandDecisionInbox(model: model, manageComputers: manageComputers, active: active, focus: focus)
                                    .padding(.horizontal, 20)
                            }
                        }
                    }.scrollIndicators(.hidden)
                }
            }
            if showsCallControls {
                CallControlsView(call: call, machine: model.selected, project: model.selectedProject,
                                 typing: typing, toggleText: {
                    withAnimation(reduceMotion ? .easeOut(duration: 0.12) : .snappy(duration: 0.25)) { typing.toggle() }
                })
            }
        }
            .background(IdeaPalette.background.ignoresSafeArea())
            .foregroundStyle(IdeaPalette.ink).tint(IdeaPalette.blue)
            .accessibilityElement(children: .contain).accessibilityIdentifier("norma-private-screen")
            .navigationTitle("Norma").navigationBarTitleDisplayMode(.inline)
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

    private var replyText: String {
        if call.isCalling { return call.subtitles && !call.subtitle.isEmpty ? call.subtitle : "我在这里，随时可以插话。" }
        if !cloudChat.partialReply.isEmpty { return cloudChat.partialReply }
        return cloudChat.messages.last(where: { $0.role == .norma })?.text ?? "一起，把想法变成现实。"
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
                    Button("方向与计划记录", systemImage: "list.bullet.clipboard") { showingPlans = true }.accessibilityIdentifier("command-project-plans")
                }
                Button("讨论草稿", systemImage: "note.text") { showingDiscussions = true }.accessibilityIdentifier("realtime-discussions")
                Button("任务操作记录", systemImage: "clock.arrow.circlepath") { showingOperations = true }.accessibilityIdentifier("realtime-operations")
                Button("切换或管理电脑", systemImage: "laptopcomputer", action: manageComputers)
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: typing || project == nil ? "person.crop.circle" : "laptopcomputer")
                        .font(.title3).foregroundStyle(IdeaPalette.secondary)
                        .frame(width: 40, height: 40)
                        .background(IdeaPalette.softGray, in: RoundedRectangle(cornerRadius: 12))
                    VStack(alignment: .leading, spacing: 4) {
                        Text(typing ? "私人对话" : project?.displayName ?? "私人对话")
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(2).multilineTextAlignment(.leading)
                        Text(typing || project == nil ? "仅你与 Norma" : "语音通话项目")
                            .font(.caption).foregroundStyle(IdeaPalette.secondary)
                    }
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.down").font(.caption2)
                }.frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
            }.accessibilityLabel("对话上下文")
                .accessibilityValue(typing ? "私人对话" : project?.displayName ?? "私人对话")
                .accessibilityIdentifier("realtime-project")
            Spacer(minLength: 0)
            Button { call.showSettings = true } label: {
                Image(systemName: "slider.horizontal.3").font(.body).frame(width: 44, height: 44)
                    .background(.white, in: Circle())
                    .overlay(Circle().strokeBorder(IdeaPalette.softGray, lineWidth: 1))
            }.accessibilityLabel("通话设置").accessibilityIdentifier("realtime-settings")
        }.buttonStyle(IdeaPressStyle()).padding(12)
            .background {
                let shape = RoundedRectangle(cornerRadius: 22)
                if reduceTransparency { shape.fill(.white) }
                else if #available(iOS 26.0, *) { Color.clear.glassEffect(.regular, in: shape) }
                else { shape.fill(.thinMaterial) }
            }
            .padding(.horizontal, 20).padding(.vertical, 4)
    }

}

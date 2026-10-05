import SwiftUI
import NormaCore

struct WorkspaceExecutionDetailView: View {
    let event: WorkspaceChatEvent
    let agentName: String
    let projectName: String
    let computers: ComputerModel
    let cloud: CloudChatModel
    let canFollowUp: Bool
    let followUp: (WorkspaceAgentTarget) -> Void
    @State private var tab = "terminal"
    @State private var terminal = WorkspaceTerminalModel()
    @State private var fontSize: CGFloat = 13
    @State private var followingLatest = true
    @State private var returnGeneration = 0
    @State private var retry = 0
    @State private var copied = false
    @State private var reading = TerminalGridReadingPosition()
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var phase
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    private var compact: Bool { verticalSizeClass == .compact }
    private var target: WorkspaceAgentTarget? { event.payload.target }
    private var unavailableReason: String? {
        if target == nil { return "这条成果没有关联终端；结果仍可阅读。" }
        if target?.runID == nil { return "等待任务绑定原 Agent 运行，绑定后会自动显示。" }
        if !computers.machines.contains(where: { $0.id == event.macID }) { return "原 Mac 未配对或已移除；结果仍可阅读。" }
        return terminal.failure
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text(agentName).font(compact ? .subheadline.weight(.semibold) : .headline)
                        if compact { Text(projectName).font(.caption).foregroundStyle(IdeaPalette.secondary).lineLimit(1) }
                        Spacer()
                        Text(workspaceChatStatus(event.payload.state ?? event.kind)).font(.caption).foregroundStyle(IdeaPalette.secondary)
                    }
                    if !compact { Text(projectName).font(.subheadline).foregroundStyle(IdeaPalette.secondary) }
                    Picker("执行详情", selection: $tab) { Text("终端").tag("terminal"); Text("结果").tag("result") }
                        .pickerStyle(.segmented).accessibilityIdentifier("execution-detail-tabs")
                }.padding(.horizontal, 20).padding(.vertical, compact ? 8 : 20)
                if tab == "terminal" { terminalContent }
                else { resultContent }
            }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink).tint(IdeaPalette.blue).buttonStyle(IdeaPressStyle())
                .navigationTitle("执行详情").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { dismiss() } } }
        }.presentationDetents([.large]).presentationDragIndicator(.visible)
            .task(id: "\(phase)-\(tab)-\(target?.runID ?? "none")-\(target?.sessionID ?? "none")-\(retry)") {
                terminal.stop()
                guard phase == .active, tab == "terminal", let target, target.runID != nil,
                      let machine = computers.machines.first(where: { $0.id == event.macID }) else { return }
                let vault = computers.vault
                let client = WorkspaceTerminalClient(machine: machine, mobile: computers.client) {
                    await cloud.load()
                    guard let credential = await cloud.credential else { throw MobileError.unavailable }
                    return try await collaborationRelayClient().connect(machine, cloud: credential, vault: vault)
                }
                await terminal.watch(client, request: .init(workspaceID: event.workspaceID, target: target))
            }.onDisappear { terminal.stop() }
            .onChange(of: followingLatest) { _, follows in if !follows { terminal.pauseReading() } }
    }
    private var terminalContent: some View {
        VStack(spacing: 0) {
            HStack {
                Label(terminal.history != nil ? "历史回看" : terminal.connected ? "实时终端 · 只读" : unavailableReason != nil ? "终端未连接 · 只读" : "连接中 · 只读", systemImage: "terminal")
                    .font(.caption).foregroundStyle(IdeaPalette.secondary)
                Spacer()
                Button { retry += 1 } label: { Image(systemName: "arrow.clockwise").frame(width: 44, height: 44) }.accessibilityLabel("重新连接终端")
            }.padding(.horizontal, 20)
            Text("会话实时画面，可能包含后续任务").font(.caption).foregroundStyle(IdeaPalette.secondary).padding(.horizontal, 20).padding(.bottom, 12)
            if let frame = terminal.displayed {
                WorkspaceTerminalGrid(frame: frame, fontSize: fontSize, followLatest: $followingLatest, returnGeneration: returnGeneration, reading: reading)
                    .accessibilityIdentifier("workspace-terminal-grid")
                    .overlay(alignment: .bottomTrailing) {
                        if !followingLatest || terminal.history != nil {
                            Button("回到实时", systemImage: "arrow.down") { terminal.returnToLive(); followingLatest = true; returnGeneration += 1 }
                                .padding(12).background(.regularMaterial, in: Capsule()).padding(12)
                                .accessibilityIdentifier("workspace-terminal-live")
                        }
                    }
            } else {
                ContentUnavailableView(target?.runID == nil ? "等待 Agent 启动" : unavailableReason != nil ? "终端暂不可用" : "正在连接终端", systemImage: "terminal",
                    description: Text(unavailableReason ?? "读取 Mac 上的原终端会话。"))
            }
            if let failure = terminal.failure {
                Text(failure).font(.footnote).foregroundStyle(.orange).padding(12).frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("workspace-terminal-notice")
            }
            HStack {
                Button(terminal.loadingHistory ? "读取中…" : "更早的输出") { followingLatest = false; terminal.pauseReading(); terminal.loadOlder() }
                    .frame(minHeight: 44)
                    .disabled(terminal.displayed?.startLine == 0 || terminal.displayed == nil || !terminal.connected || terminal.loadingHistory)
                    .accessibilityIdentifier("workspace-terminal-history")
                Spacer()
                Button { fontSize = max(10, fontSize - 1) } label: { Image(systemName: "textformat.size.smaller").frame(width: 44, height: 44) }.accessibilityLabel("缩小终端字号").disabled(fontSize <= 10)
                Button { fontSize = min(24, fontSize + 1) } label: { Image(systemName: "textformat.size.larger").frame(width: 44, height: 44) }.accessibilityLabel("放大终端字号").disabled(fontSize >= 24)
                Button { UIPasteboard.general.string = terminal.displayed?.text(); copied = true } label: { Image(systemName: copied ? "checkmark" : "doc.on.doc").frame(width: 44, height: 44) }
                    .disabled(terminal.displayed == nil).accessibilityLabel(copied ? "已复制终端输出" : "复制终端输出")
                    .accessibilityIdentifier("workspace-terminal-copy")
            }.font(.caption).padding(.horizontal, 16).frame(minHeight: 48).background(.regularMaterial)
        }.frame(maxHeight: .infinity)
    }
    private var resultContent: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if event.kind == "task_result" || ["succeeded","failed","interrupted","cancelled"].contains(event.payload.state ?? "") {
                    WorkspaceResultTextView(text: event.payload.text ?? "Mac 尚未提供正文结果。")
                } else { Label("任务正在执行，完成后会显示结果。", systemImage: "clock").foregroundStyle(IdeaPalette.secondary) }
                if let target {
                    DisclosureGroup("执行详情") {
                        Text("任务：\(event.payload.taskRef ?? "无")\n运行：\(target.runID ?? "尚未绑定")\n会话：\(target.sessionID ?? "尚未绑定")")
                            .font(.footnote.monospaced()).textSelection(.enabled)
                    }
                    Button("追问这个任务") { followUp(target) }.frame(minHeight: 48)
                        .disabled(!canFollowUp || target.runID == nil || target.sessionID == nil).accessibilityIdentifier("group-followup")
                }
                if let error = event.payload.error { Text(WorkspaceChatError.server(error).localizedDescription).foregroundStyle(.red) }
            }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
        }.accessibilityIdentifier("workspace-execution-result")
    }
}

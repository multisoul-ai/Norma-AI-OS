import SwiftUI
import NormaCore

struct ArchivedConversationView: View {
    let model: ComputerModel
    let machine: PairedMachine
    let scope: ProjectReviewRequest
    let session: ArchivedSession
    @Environment(\.scenePhase) private var phase
    @State private var timeline = ConversationTimeline()
    @State private var cursor: String?
    @State private var revision: UInt64?
    @State private var retry = 0
    @State private var loading = false
    @State private var error: String?
    @State private var pendingTail = false

    private var more: some View {
        Button(session.key.profile == "Codex" ? "加载更早的消息" : "继续加载消息") { cursor = timeline.cursor }
            .font(.subheadline).frame(minHeight: 44).disabled(loading || phase != .active)
    }
    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 5) {
                    Label(machine.name, systemImage: "laptopcomputer")
                    Label("会话记录 · 只读", systemImage: "clock")
                }.font(.caption).foregroundStyle(.secondary)
                if let error {
                    Text(error).font(.subheadline).foregroundStyle(.orange)
                    Text("需要电脑在线且项目保持打开；原始记录被清理后，将无法读取。")
                        .font(.caption).foregroundStyle(.secondary)
                }
                if session.key.profile == "Codex", timeline.hasMore { more }
                ForEach(timeline.items) { item in
                    ConversationMessageRow(item: item, profile: session.key.profile)
                }
                if session.key.profile == "Claude", timeline.hasMore { more }
                if loading { ProgressView("正在读取原始对话…") }
                if !loading && timeline.items.isEmpty && error == nil {
                    ContentUnavailableView("暂时没有消息", systemImage: "bubble.left", description: Text("会话已登记，Agent 尚未写入可读消息。"))
                }
                if pendingTail { Text("电脑正在写入下一条消息，请稍后刷新。") .font(.caption).foregroundStyle(.secondary) }
            }.padding(16)
        }
        .background(.black).navigationTitle(session.title).navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { cursor = nil; revision = nil; retry += 1 } label: { Image(systemName: "arrow.clockwise") }
                    .disabled(loading || phase != .active).accessibilityLabel("刷新原始对话")
            }
        }
        .task(id: "\(phase)-\(retry)-\(cursor ?? "first")") {
            if cursor == nil { timeline = ConversationTimeline(); pendingTail = false }
            loading = false; error = nil
            guard phase == .active else { timeline = ConversationTimeline(); cursor = nil; revision = nil; return }
            loading = true
            var loaded = false
            do {
                for try await update in model.client.projectUpdates(machine, workspaceID: scope.workspaceID) {
                    try Task.checkCancellation()
                    switch update {
                    case .recovering: throw MobileError.connectionFailed
                    case .current(let snapshot):
                        guard let project = snapshot.projects.first, project.state == "open", project.routeID == scope.routeID else { throw MobileError.sessionChanged }
                        if let revision, revision != snapshot.grant.revision { throw MobileError.grantChanged }
                        revision = snapshot.grant.revision
                        guard !loaded else { continue }
                        let result = try await model.client.archivedConversation(machine, request: session.conversationRequest(scope: scope, cursor: cursor), profile: session.key.profile)
                        try Task.checkCancellation()
                        timeline.merge(result.page, continuing: cursor != nil)
                        pendingTail = result.page.pendingTail; loading = false; loaded = true
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                timeline = ConversationTimeline(); pendingTail = false; loading = false; self.error = error.localizedDescription
            }
        }
    }
}

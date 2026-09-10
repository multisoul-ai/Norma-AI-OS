import SwiftUI
import NormaCore

struct SessionArchiveView: View {
    let model: ComputerModel
    let machine: PairedMachine
    let scope: ProjectReviewRequest
    @Environment(\.scenePhase) private var phase
    @State private var items: [ArchivedSession] = []
    @State private var next: UInt64?
    @State private var before: UInt64?
    @State private var revision: UInt64?
    @State private var selected: String?
    @State private var retry = 0
    @State private var loading = false
    @State private var error: String?

    var body: some View {
        List {
            Section {
                Label(machine.name, systemImage: "laptopcomputer").font(.caption).foregroundStyle(.secondary)
            } footer: {
                Text("删除卡片后，对话仍保留在电脑上。这里可查看接入手机控制后登记的会话。")
            }
            if let error { Text(error).font(.subheadline).foregroundStyle(.orange) }
            ForEach(items) { item in
                Button { selected = item.id } label: {
                    HStack(spacing: 12) {
                        Image(systemName: "bubble.left.and.text.bubble.right").foregroundStyle(.secondary)
                        VStack(alignment: .leading, spacing: 4) {
                            Text(item.title).font(.subheadline.weight(.medium)).foregroundStyle(.primary)
                            Text(item.key.profile + " · " + Date(timeIntervalSince1970: TimeInterval(item.recordedAtUnix)).formatted(date: .abbreviated, time: .shortened))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                        Image(systemName: "chevron.right").font(.caption2).foregroundStyle(.tertiary)
                    }.frame(minHeight: 44)
                }.accessibilityIdentifier("archived-session-\(item.id)")
            }
            if loading { ProgressView("正在读取会话…") }
            if !loading && error == nil && items.isEmpty {
                ContentUnavailableView("还没有会话记录", systemImage: "bubble.left.and.bubble.right", description: Text("Agent 开始对话后，记录会显示在这里。"))
            }
            if let next { Button("更早的会话") { before = next }.frame(minHeight: 44).disabled(loading) }
        }
        .scrollContentBackground(.hidden).background(.black)
        .navigationTitle("会话记录").navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { before = nil; revision = nil; retry += 1 } label: { Image(systemName: "arrow.clockwise") }
                    .disabled(loading || phase != .active).accessibilityLabel("刷新会话记录")
            }
        }
        .navigationDestination(item: $selected) { id in
            if let item = items.first(where: { $0.id == id }) {
                ArchivedConversationView(model: model, machine: machine, scope: scope, session: item)
            }
        }
        .task(id: "\(phase)-\(retry)-\(before.map(String.init) ?? "first")") {
            if before == nil { items = []; next = nil; selected = nil }
            error = nil; loading = false
            guard phase == .active else { items = []; selected = nil; before = nil; revision = nil; return }
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
                        let page = try await model.client.sessionArchive(machine, scope: scope, before: before)
                        try Task.checkCancellation()
                        if before == nil { items = page.items } else { items += page.items }
                        next = page.nextCursor; loading = false; loaded = true
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                items = []; next = nil; selected = nil; loading = false; self.error = error.localizedDescription
            }
        }
    }
}

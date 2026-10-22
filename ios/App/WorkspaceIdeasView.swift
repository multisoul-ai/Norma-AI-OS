import SwiftUI
import NormaCore

/// 电脑端工作空间的想法清单，只读：手机不能新增、编辑或指派，要记新想法请用“我的想法”。
struct WorkspaceIdeasView: View {
    let model: ComputerModel
    let machine: PairedMachine
    let workspaceID: String
    let name: String
    @Environment(\.scenePhase) private var scenePhase
    @State private var ideas: [WorkspaceIdea] = []
    @State private var error: String?
    @State private var loading = false
    @State private var loaded = false

    private func group(_ state: WorkspaceIdea.State) -> [WorkspaceIdea] { ideas.filter { $0.state == state } }

    var body: some View {
        List {
            if let error {
                Text(error).font(.caption).foregroundStyle(.orange)
            } else if loaded && ideas.isEmpty {
                ContentUnavailableView("还没有想法", systemImage: "lightbulb", description: Text("在电脑上对 Norma 说“记一下……”，这里就会出现。"))
            }
            section("待办", group(.todo), symbol: "circle")
            section("已指派", group(.assigned), symbol: "arrow.right.circle")
            section("已完成", group(.done), symbol: "checkmark.circle")
        }
        .scrollContentBackground(.hidden).background(.black)
        .workspaceLoading(loading && !loaded, "正在读取想法…")
        .navigationTitle("想法清单").navigationBarTitleDisplayMode(.inline)
        .refreshable { await load() }
        .task(id: scenePhase) { if scenePhase == .active { await load() } }
    }

    @ViewBuilder private func section(_ title: String, _ rows: [WorkspaceIdea], symbol: String) -> some View {
        if !rows.isEmpty {
            Section {
                ForEach(rows) { idea in
                    Label(idea.title, systemImage: symbol).font(.subheadline).padding(.vertical, 4)
                        .accessibilityLabel("\(title)：\(idea.title)")
                }
            } header: { Text("\(title) · \(rows.count)") }
        }
    }

    private func load() async {
        loading = true
        defer { loading = false }
        do {
            let fresh = try await model.client.workspaceIdeas(machine, workspaceID: workspaceID)
            try Task.checkCancellation()
            ideas = fresh; error = nil; loaded = true
        } catch { if !Task.isCancelled { self.error = error.localizedDescription } }
    }
}

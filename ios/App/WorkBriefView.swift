import SwiftUI
import NormaCore

struct WorkBriefView: View {
    @Bindable var library: WorkspaceLibraryModel
    let thread: WorkThread
    @State private var facts: [WorkspaceFact] = []
    @State private var editing = false
    @State private var kind: WorkspaceFactKind = .goal
    @State private var previous: WorkspaceFact?
    @State private var text = ""
    @State private var saving = false
    @State private var loaded = false
    @State private var error: String?
    @State private var history: [WorkspaceFact] = []
    @State private var showingHistory = false
    @State private var requiresRefresh = false
    @State private var refreshedText: String?
    private var workspace: CollaborationWorkspace? { library.workspaces.first { $0.id == thread.workspaceID } }
    private var goal: WorkspaceFact? { facts.first { $0.kind == .goal } }
    private var writable: Bool { loaded && workspace?.archived == false }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text(loaded ? "保存在本机 · 尚未同步到 Mac" : "正在核对记录；读取失败时保留上次内容")
                    .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                VStack(alignment: .leading, spacing: 15) {
                    Label("当前目标", systemImage: "scope").font(.headline)
                    Text(goal?.text ?? "还没有设置这件事的目标").font(.title2.bold())
                    Button(goal == nil ? "设置目标" : "修改目标") { edit(.goal, goal) }
                        .buttonStyle(.bordered).foregroundStyle(IdeaPalette.blue)
                        .frame(minHeight: 44).accessibilityIdentifier("brief-edit-goal").disabled(!writable)
                    if let goal { historyButton(goal) }
                }.threadCard()
                VStack(alignment: .leading, spacing: 14) {
                    Label("关联项目", systemImage: "folder").font(.headline)
                    if workspace?.projects.isEmpty != false {
                        Text("尚未关联项目，可在工作空间设置中添加。")
                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    }
                    ForEach(workspace?.projects ?? []) { project in
                        Label("\(project.name) · \(project.machineName)", systemImage: "folder.fill").font(.subheadline)
                            .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                            .background(IdeaPalette.blue.opacity(0.07), in: RoundedRectangle(cornerRadius: 14))
                    }
                }.threadCard()
                factSection(.decision, title: "共同决定", symbol: "doc.text")
                factSection(.fact, title: "共同事实", symbol: "text.book.closed")
                if let error { Text(error).foregroundStyle(.red) }
                if !loaded || error != nil { Button("重新读取") { Task { await load() } }.frame(minHeight: 44) }
            }.padding(20)
        }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink)
            .navigationTitle("工作简报").navigationBarTitleDisplayMode(.inline).tint(IdeaPalette.blue)
            .task { await load() }
            .sheet(isPresented: $editing) { editor }
            .sheet(isPresented: $showingHistory) { historySheet }
    }

    private func factSection(_ kind: WorkspaceFactKind, title: String, symbol: String) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Label(title, systemImage: symbol).font(.headline)
            Text("同一工作空间内共享，目标仍各自独立。")
                .font(.footnote).foregroundStyle(IdeaPalette.secondary)
            ForEach(facts.filter { $0.kind == kind }) { fact in
                VStack(alignment: .leading, spacing: 8) {
                    Text(fact.text).font(.body)
                    HStack {
                        historyButton(fact)
                        Spacer()
                        Button("修改") { edit(kind, fact) }.frame(minHeight: 44).disabled(!writable)
                            .foregroundStyle(IdeaPalette.blue)
                            .accessibilityLabel("修改\(title)：\(fact.text)")
                    }
                }.padding(14).background(IdeaPalette.softGray, in: RoundedRectangle(cornerRadius: 16))
            }
            Button { edit(kind, nil) } label: { Label("添加\(title)", systemImage: "plus").frame(minHeight: 44) }
                .foregroundStyle(IdeaPalette.blue)
                .disabled(!writable).accessibilityIdentifier("brief-add-\(kind.rawValue)")
        }.threadCard()
    }
    private func historyButton(_ fact: WorkspaceFact) -> some View {
        Button {
            Task {
                do { history = try await library.store.factHistory(id: fact.id); showingHistory = true }
                catch { self.error = error.localizedDescription }
            }
        } label: {
            Label("版本 \(fact.version) · \(fact.sourceMessageID == nil ? "手动修改" : "来自对话")", systemImage: "clock.arrow.circlepath")
                .font(.caption).frame(minHeight: 44)
        }.foregroundStyle(IdeaPalette.blue).accessibilityIdentifier("brief-history-\(fact.id)")
    }
    private var editor: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text(kind == .goal ? "只影响当前线程的目标。" : "这条内容会在当前工作空间内共享。")
                    .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                TextEditor(text: $text).padding(8).scrollContentBackground(.hidden)
                    .background(.white, in: RoundedRectangle(cornerRadius: 18)).accessibilityIdentifier("brief-fact-editor")
                    .accessibilityLabel(kind == .goal ? "当前目标内容" : kind == .decision ? "共同决定内容" : "共同事实内容")
                    .disabled(saving)
                if let error { Text(error).foregroundStyle(.red).font(.subheadline) }
                if requiresRefresh {
                    Button("读取最新版本，保留我的草稿") { Task { await refreshEditingBase() } }
                        .frame(minHeight: 44).disabled(saving)
                }
                if let refreshedText {
                    Text("刚读到的版本：\(refreshedText)").font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                }
            }.padding(20).background(IdeaPalette.background)
                .navigationTitle(kind == .goal ? "当前目标" : kind == .decision ? "共同决定" : "共同事实")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("取消") { editing = false }.disabled(saving) }
                    ToolbarItem(placement: .confirmationAction) {
                        Button(saving ? "保存中…" : "保存") { Task { await save() } }
                            .disabled(saving || requiresRefresh || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                            .accessibilityIdentifier("brief-fact-save")
                    }
                }
        }.presentationDetents([.medium, .large]).presentationDragIndicator(.visible).interactiveDismissDisabled(saving)
    }
    private var historySheet: some View {
        NavigationStack {
            List(history.reversed()) { fact in
                VStack(alignment: .leading, spacing: 10) {
                    Text("版本 \(fact.version) · 你").font(.headline)
                    Text(fact.text).textSelection(.enabled)
                    Text(fact.createdAt, format: .dateTime.year().month().day().hour().minute())
                        .font(.caption).foregroundStyle(IdeaPalette.secondary)
                    if let source = fact.sourceMessageID {
                        SourceMessageLabel(store: library.store, workspaceID: fact.workspaceID, messageID: source)
                    } else { Text("手动修改").font(.caption).foregroundStyle(IdeaPalette.secondary) }
                }.padding(.vertical, 6)
            }.navigationTitle("修改历史").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { showingHistory = false } } }
        }.presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
    }
    private func edit(_ value: WorkspaceFactKind, _ existing: WorkspaceFact?) {
        kind = value; previous = existing; text = existing?.text ?? ""; error = nil; editing = true
        requiresRefresh = false; refreshedText = nil
    }
    private func load() async {
        do { facts = try await library.store.facts(threadID: thread.id); loaded = true; error = nil }
        catch { loaded = false; self.error = error.localizedDescription }
    }
    private func save() async {
        guard !saving else { return }
        saving = true
        defer { saving = false }
        do {
            _ = try await library.store.saveFact(threadID: thread.id, kind: kind, text: text, replacing: previous)
            editing = false; await load()
        } catch {
            if case WorkThreadError.conflict = error { requiresRefresh = true }
            self.error = error.localizedDescription
        }
    }
    private func refreshEditingBase() async {
        saving = true
        defer { saving = false }
        do {
            let current = try await library.store.facts(threadID: thread.id)
            let previousID = previous?.id
            guard let latest = current.first(where: { previousID == nil ? $0.kind == .goal && kind == .goal : $0.id == previousID }) else {
                throw WorkThreadError.notFound
            }
            previous = latest
            refreshedText = previous?.text
            facts = current; loaded = true; requiresRefresh = false; error = nil
        } catch { self.error = error.localizedDescription }
    }
}

private struct SourceMessageLabel: View {
    let store: CollaborationWorkspaceStore
    let workspaceID: String
    let messageID: String
    @State private var text = "正在读取来源…"
    var body: some View {
        Text("来源消息：\(text)").font(.subheadline).foregroundStyle(IdeaPalette.secondary).textSelection(.enabled)
            .task {
                do { text = try await store.sourceMessage(id: messageID, workspaceID: workspaceID).text }
                catch { text = error.localizedDescription }
            }
    }
}

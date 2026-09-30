import SwiftUI
import NormaCore

struct WorkspaceThreadListView: View {
    @Bindable var library: WorkspaceLibraryModel
    let computers: ComputerModel
    let workspaceID: String
    @Environment(\.dismiss) private var dismiss
    @State private var creating = false
    @State private var configuring = false
    @State private var selected: WorkThread?
    @State private var created: WorkThread?
    private var workspace: CollaborationWorkspace? { library.workspaces.first { $0.id == workspaceID } }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    HStack {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(workspace?.name ?? "工作空间").font(.largeTitle.bold())
                            Text(workspace?.archived == true ? "已归档 · 历史仍保留" : "每件事有独立的对话与目标")
                                .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                        }
                        Spacer(minLength: 0)
                    }
                    if workspace?.archived == false {
                        Button { creating = true } label: {
                            Label("新建工作线程", systemImage: "plus").font(.headline)
                                .frame(maxWidth: .infinity, minHeight: 50).foregroundStyle(.white)
                                .background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 16))
                        }.accessibilityIdentifier("create-work-thread")
                    }
                    ForEach(library.threads[workspaceID] ?? []) { thread in
                        Button { selected = thread } label: {
                            HStack(spacing: 12) {
                                Image(systemName: "text.bubble").font(.title2).foregroundStyle(IdeaPalette.blue)
                                VStack(alignment: .leading, spacing: 8) {
                                    Text(thread.title).font(.headline)
                                    Text(thread.sourceExcerpt ?? "独立工作线程").font(.subheadline)
                                        .foregroundStyle(IdeaPalette.secondary).lineLimit(2)
                                }
                                Spacer(minLength: 0)
                                Image(systemName: "chevron.right").foregroundStyle(IdeaPalette.secondary)
                            }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
                                .background(.white, in: RoundedRectangle(cornerRadius: 22))
                        }.accessibilityIdentifier("work-thread-row-\(thread.id)")
                    }
                    if let error = library.error {
                        Text(error).foregroundStyle(.red).font(.subheadline)
                        Button("重新读取") { Task { await library.load() } }.frame(minHeight: 44)
                    } else if !library.loaded {
                        ProgressView("正在读取工作线程…")
                    } else if library.threads[workspaceID]?.isEmpty != false {
                        ContentUnavailableView("还没有工作线程", systemImage: "text.bubble",
                                               description: Text("新建一件事，或从想法库选择一条想法来推进。"))
                    }
                }.padding(20)
            }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink).buttonStyle(IdeaPressStyle())
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button { dismiss() } label: { Image(systemName: "chevron.left") }
                            .accessibilityLabel("返回工作空间列表").accessibilityIdentifier("workspace-thread-list-back")
                    }
                    ToolbarItem(placement: .topBarTrailing) {
                        Button { configuring = true } label: { Image(systemName: "slider.horizontal.3") }
                            .accessibilityLabel("工作空间设置").accessibilityIdentifier("thread-workspace-settings")
                    }
                }
                .navigationDestination(item: $selected) { WorkThreadView(library: library, thread: $0) }
                .sheet(isPresented: $creating, onDismiss: { selected = created; created = nil }) {
                    NewWorkThreadView(library: library, workspaceID: workspaceID) { created = $0 }
                }
                .fullScreenCover(isPresented: $configuring) {
                    if let workspace { WorkspaceSettingsView(library: library, computers: computers, workspace: workspace) }
                }
                .task { await library.load() }
        }.tint(IdeaPalette.blue).preferredColorScheme(.light)
    }
}

struct NewWorkThreadView: View {
    @Bindable var library: WorkspaceLibraryModel
    var workspaceID: String?
    var source: Idea?
    let created: (WorkThread) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var selection: String?
    @State private var title = ""
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                Section("这次推进什么") {
                    TextField("线程标题", text: $title).accessibilityIdentifier("thread-title")
                }
                if !library.loaded && library.error == nil { ProgressView("正在读取工作空间…") }
                if let loadError = library.error {
                    Text(loadError).foregroundStyle(.red)
                    Button("重新读取工作空间") { Task { await library.load() } }.frame(minHeight: 44)
                }
                if workspaceID == nil {
                    Section("选择工作空间") {
                        ForEach(library.workspaces.filter { !$0.archived }) { workspace in
                            Button { selection = workspace.id } label: {
                                HStack {
                                    Text(workspace.name)
                                    Spacer()
                                    Image(systemName: selection == workspace.id ? "checkmark.circle.fill" : "circle")
                                }.frame(minHeight: 44)
                            }.accessibilityIdentifier("thread-workspace-option-\(workspace.id)")
                                .accessibilityValue(selection == workspace.id ? "已选择" : "未选择")
                        }
                        if library.loaded && library.error == nil && library.workspaces.allSatisfy(\.archived) {
                            Text("还没有可用的工作空间。请先在工作空间页新建一个，再来推进这条想法。")
                                .foregroundStyle(IdeaPalette.secondary)
                        }
                    }
                }
                if let source {
                    Section("来源想法") {
                        Text(source.body.isEmpty ? "语音想法，待转写" : source.body)
                        Text("原想法继续保留在想法库。这里只保存一份引用。")
                            .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                    }
                }
                if let error { Text(error).foregroundStyle(.red) }
            }.disabled(saving).navigationTitle("新建工作线程").navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("取消") { dismiss() }.disabled(saving) }
                    ToolbarItem(placement: .confirmationAction) {
                        Button(saving ? "保存中…" : "创建") { Task { await create() } }
                            .disabled(saving || selection == nil || library.error != nil || title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                            .accessibilityIdentifier("thread-create-save")
                    }
                }
                .task {
                    selection = workspaceID
                    if let source { title = String(source.body.prefix(40)) }
                    await library.load()
                }
        }.tint(IdeaPalette.blue).presentationDetents([.large]).presentationDragIndicator(.visible)
            .interactiveDismissDisabled(saving)
    }

    private func create() async {
        guard let selection, !saving else { return }
        saving = true
        defer { saving = false }
        do {
            let thread = try await library.store.createThread(workspaceID: selection, title: title, source: source)
            await library.load()
            created(thread); dismiss()
        } catch { self.error = error.localizedDescription }
    }
}

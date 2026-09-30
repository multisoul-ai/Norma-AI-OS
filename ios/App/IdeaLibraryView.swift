import SwiftUI
import NormaCore

private enum IdeaRoute: Identifiable {
    case text(Idea?)
    case voice
    case recovery(URL)

    var id: String {
        switch self {
        case .text(let idea): "text-\(idea?.id ?? "new")"
        case .voice: "voice-new"
        case .recovery(let url): "recovery-\(url.lastPathComponent)"
        }
    }
}

struct IdeaLibraryView: View {
    @Bindable var model: IdeaLibraryModel
    @Bindable var workspaces: WorkspaceLibraryModel
    let navigateToTab: (Int) -> Void
    let openComputers: () -> Void
    let openSettings: () -> Void
    var showsTabBar = true
    @State private var route: IdeaRoute?
    @State private var choosingKind = false
    @State private var showingDrawer = false
    @State private var showingSearch = false
    @State private var query = ""
    @State private var newestFirst = true
    @State private var savedFeedback = false
    @State private var feedbackTask: Task<Void, Never>?
    @State private var advancing: Idea?
    @State private var createdThread: WorkThread?
    @State private var openedThread: WorkThread?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.dynamicTypeSize) private var typeSize

    private var visibleIdeas: [Idea] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let ideas = term.isEmpty ? model.ideas : model.ideas.filter {
            $0.body.localizedStandardContains(term) || ($0.rawTranscript?.localizedStandardContains(term) ?? false)
        }
        return ideas.sorted { newestFirst ? $0.createdAt > $1.createdAt : $0.createdAt < $1.createdAt }
    }

    var body: some View {
        ZStack {
            IdeaPalette.background.ignoresSafeArea()
            decorations
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    header
                    titleRow
                    Text("随时记录闪现的想法，让好点子不再丢失。")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                        .padding(.top, 9)
                    if let recording = model.recoverableRecordings.first {
                        Button { route = .recovery(recording) } label: {
                            Label("有一段未保存录音，点击恢复", systemImage: "waveform.badge.mic")
                                .font(.subheadline.weight(.medium))
                                .frame(maxWidth: .infinity, minHeight: 48)
                                .background(IdeaPalette.mint.opacity(0.18), in: RoundedRectangle(cornerRadius: 15))
                        }
                        .accessibilityIdentifier("recover-idea-recording")
                        .padding(.top, 16)
                    }
                    listHeader.padding(.top, 29)
                    if showingSearch {
                        HStack(spacing: 10) {
                            Image(systemName: "magnifyingglass").foregroundStyle(IdeaPalette.secondary)
                            TextField("搜索想法", text: $query)
                                .accessibilityIdentifier("idea-search-field")
                                .textInputAutocapitalization(.never)
                            if !query.isEmpty {
                                Button { query = "" } label: { Image(systemName: "xmark.circle.fill") }
                                    .accessibilityLabel("清除搜索")
                            }
                        }
                        .padding(.horizontal, 15).frame(minHeight: 48)
                        .background(.white, in: RoundedRectangle(cornerRadius: 17))
                        .padding(.top, 14)
                        .transition(reduceMotion ? .opacity : .move(edge: .top).combined(with: .opacity))
                    }
                    if !model.loaded, let error = model.error {
                        VStack(spacing: 12) {
                            Text(error).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                            Button("重新读取") { Task { await model.load() } }
                                .frame(minHeight: 44)
                        }.frame(maxWidth: .infinity).padding(.top, 35)
                    } else if !model.loaded {
                        ProgressView("正在读取想法…").frame(maxWidth: .infinity).padding(.top, 45)
                    } else if visibleIdeas.isEmpty {
                        emptyState.padding(.top, 28)
                    } else {
                        LazyVStack(spacing: 14) {
                            ForEach(visibleIdeas) { idea in
                                IdeaCard(idea: idea, open: { route = .text(idea) }, advance: { advancing = idea })
                            }
                        }.padding(.top, 12)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.top, 13)
                .padding(.bottom, 28)
            }
            .scrollIndicators(.hidden)
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if showsTabBar { WorkspaceTabBar(selection: Binding(get: { 0 }, set: { navigateToTab($0) })) }
            }
            if savedFeedback {
                Label("已记录", systemImage: "checkmark.circle.fill")
                    .font(.subheadline.weight(.semibold))
                    .padding(.horizontal, 25).frame(height: 51)
                    .foregroundStyle(.white)
                    .background(Color(red: 0.19, green: 0.21, blue: 0.25), in: Capsule())
                    .shadow(color: .black.opacity(0.17), radius: 14, y: 7)
                    .frame(maxHeight: .infinity, alignment: .bottom)
                    .padding(.bottom, 100)
                    .accessibilityIdentifier("idea-saved-feedback")
                    .transition(.opacity.combined(with: .scale(scale: 0.96)))
            }
        }
        .foregroundStyle(IdeaPalette.ink)
        .tint(IdeaPalette.blue)
        .task { await model.load() }
        .confirmationDialog("记录想法", isPresented: $choosingKind, titleVisibility: .visible) {
            Button("文字记录", systemImage: "text.alignleft") { route = .text(nil) }
                .accessibilityIdentifier("record-text-idea")
            Button("语音记录", systemImage: "mic") { route = .voice }
                .accessibilityIdentifier("record-voice-idea")
        }
        .fullScreenCover(item: $route) { route in
            switch route {
            case .text(let idea):
                IdeaTextEditorView(model: model, editing: idea) { saved() }
                    .preferredColorScheme(.light)
            case .voice:
                IdeaVoiceView(model: model) { saved() }
                    .preferredColorScheme(.light)
            case .recovery(let url):
                IdeaVoiceView(model: model, recoveringURL: url) { saved() }
                    .preferredColorScheme(.light)
            }
        }
        .sheet(isPresented: $showingDrawer) { moduleDrawer.presentationDetents([.medium]) }
        .sheet(item: $advancing, onDismiss: { openedThread = createdThread; createdThread = nil }) { idea in
            NewWorkThreadView(library: workspaces, source: idea) { createdThread = $0 }
        }
        .fullScreenCover(item: $openedThread) { thread in
            NavigationStack {
                WorkThreadView(library: workspaces, thread: thread)
                    .toolbar {
                        ToolbarItem(placement: .cancellationAction) {
                            Button { openedThread = nil } label: { Image(systemName: "chevron.left") }
                                .accessibilityLabel("返回想法库").accessibilityIdentifier("thread-back-to-ideas")
                        }
                    }
            }.tint(IdeaPalette.blue).preferredColorScheme(.light)
        }
        .alert("想法库", isPresented: Binding(get: { model.error != nil && route == nil }, set: { if !$0 { model.error = nil } })) {
            Button("好") { model.error = nil }
        } message: { Text(model.error ?? "") }
    }

    private var decorations: some View {
        GeometryReader { geometry in
            Circle().fill(Color.yellow.opacity(0.18)).frame(width: 92, height: 92)
                .position(x: geometry.size.width * 0.47, y: 4)
            Circle().fill(Color.pink.opacity(0.16)).frame(width: 76, height: 76)
                .position(x: geometry.size.width + 4, y: 105)
            Circle().fill(IdeaPalette.mint.opacity(0.18)).frame(width: 66, height: 66)
                .position(x: geometry.size.width + 6, y: 205)
        }.allowsHitTesting(false).accessibilityHidden(true)
    }

    private var header: some View {
        Button { showingDrawer = true } label: {
            Image(systemName: "line.3.horizontal")
                .font(.system(size: 21, weight: .medium))
                .frame(width: 48, height: 48)
                .background(IdeaPalette.softGray, in: Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("更多模块")
        .accessibilityIdentifier("idea-more-modules")
    }

    private var titleRow: some View {
        HStack(alignment: .center, spacing: 12) {
            Text("想法库").font(.largeTitle.bold()).tracking(-1.2)
                .accessibilityAddTraits(.isHeader)
            Spacer(minLength: 0)
            Button { choosingKind = true } label: {
                Label("记录想法", systemImage: "plus")
                    .font(.headline)
                    .frame(minHeight: 49)
                    .padding(.horizontal, 16)
                    .foregroundStyle(.white)
                    .background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 13))
            }
            .buttonStyle(IdeaPressStyle())
            .accessibilityIdentifier("record-idea")
        }
    }

    private var listHeader: some View {
        HStack(spacing: 9) {
            Text("全部想法").font(.headline)
            Text("\(visibleIdeas.count)").font(.subheadline)
                .padding(.horizontal, 10).frame(minHeight: 26)
                .background(IdeaPalette.softGray, in: Capsule())
            Spacer()
            Button {
                withAnimation(reduceMotion ? .easeOut(duration: 0.12) : .snappy(duration: 0.25)) {
                    showingSearch.toggle()
                    if !showingSearch { query = "" }
                }
            } label: { Image(systemName: "magnifyingglass").frame(width: 44, height: 44) }
                .accessibilityLabel("搜索想法").accessibilityIdentifier("search-ideas-toggle")
            Menu {
                Picker("排序", selection: $newestFirst) {
                    Text("最近记录").tag(true)
                    Text("最早记录").tag(false)
                }
            } label: {
                HStack(spacing: 5) {
                    Text(newestFirst ? "最近记录" : "最早记录")
                    Image(systemName: "chevron.down").font(.caption)
                }.font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    .frame(minHeight: 44)
            }.accessibilityIdentifier("idea-sort")
        }
    }

    private var emptyState: some View {
        VStack(spacing: 10) {
            Image(systemName: query.isEmpty ? "lightbulb" : "magnifyingglass")
                .font(.system(size: 33)).foregroundStyle(IdeaPalette.blue)
                .frame(width: 80, height: 80).background(.white, in: Circle())
            Text(query.isEmpty ? "先记录一个想法" : "没有找到相关想法").font(.headline)
            Text(query.isEmpty ? "文字或语音都可以，记录后不会自动开始执行。" : "试试其他关键词。")
                .font(.subheadline).foregroundStyle(IdeaPalette.secondary).multilineTextAlignment(.center)
        }.frame(maxWidth: .infinity).padding(28)
    }

    private var moduleDrawer: some View {
        NavigationStack {
            List {
                Button { showingDrawer = false; navigateToTab(0) } label: { Label("工作空间", systemImage: "folder") }
                Button { showingDrawer = false } label: { Label("想法库", systemImage: "lightbulb") }
                Button { showingDrawer = false; openComputers() } label: { Label("已连接电脑", systemImage: "laptopcomputer") }
                Button { showingDrawer = false; openSettings() } label: { Label("设置", systemImage: "gearshape") }
            }
            .navigationTitle("更多模块")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { showingDrawer = false } } }
        }.tint(IdeaPalette.blue)
    }

    private func saved() {
        route = nil
        feedbackTask?.cancel()
        withAnimation(.easeOut(duration: 0.16)) { savedFeedback = true }
        feedbackTask = Task {
            try? await Task.sleep(for: .seconds(2))
            guard !Task.isCancelled else { return }
            withAnimation(.easeOut(duration: 0.16)) { savedFeedback = false }
        }
    }
}

private struct IdeaCard: View {
    let idea: Idea
    let open: () -> Void
    let advance: () -> Void

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 10) {
                    Image(systemName: idea.kind == .voice ? "mic" : "doc.text")
                        .font(.system(size: 18, weight: .medium))
                        .frame(width: 36, height: 36)
                        .foregroundStyle(idea.kind == .voice ? IdeaPalette.green : IdeaPalette.blue)
                        .background(idea.kind == .voice ? IdeaPalette.mint.opacity(0.27) : IdeaPalette.blue.opacity(0.1), in: Circle())
                    Text(idea.kind == .voice ? "语音转写" : "文字")
                        .font(.caption)
                        .padding(.horizontal, 9).frame(minHeight: 25)
                        .background(idea.kind == .voice ? IdeaPalette.mint.opacity(0.17) : IdeaPalette.blue.opacity(0.08), in: Capsule())
                    Spacer()
                    Color.clear.frame(width: 44, height: 36)
                }
                Text(idea.body.isEmpty ? "待转写录音" : idea.body)
                    .font(.body).multilineTextAlignment(.leading)
                    .lineLimit(3).frame(maxWidth: .infinity, alignment: .leading)
                HStack(spacing: 9) {
                    Image(systemName: idea.transcriptionState == .pending || idea.transcriptionState == .failed ? "clock" : "circle.fill")
                        .foregroundStyle(IdeaPalette.faint)
                    Text(idea.transcriptionState == .pending || idea.transcriptionState == .failed ? "待转写，录音已保存" : "仅记录，尚未执行")
                }.font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                Text(createdLabel)
                    .font(.caption).foregroundStyle(IdeaPalette.faint)
            }
            .padding(18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.white, in: RoundedRectangle(cornerRadius: 25))
            .shadow(color: IdeaPalette.ink.opacity(0.035), radius: 20, y: 7)
        }
        .buttonStyle(IdeaPressStyle())
        .accessibilityLabel("\(idea.kind == .voice ? "语音想法" : "文字想法")，\(idea.body.isEmpty ? "待转写录音" : idea.body)")
        .accessibilityValue(idea.transcriptionState == .pending || idea.transcriptionState == .failed ? "待转写" : "仅记录")
        .accessibilityIdentifier("idea-card-\(idea.id)")
        .overlay(alignment: .topTrailing) {
            Menu {
                Button("编辑想法", systemImage: "pencil", action: open)
                Button("在工作空间推进", systemImage: "arrow.up.forward.app", action: advance)
            } label: {
                Image(systemName: "ellipsis").font(.headline)
                    .foregroundStyle(IdeaPalette.secondary)
                    .frame(width: 44, height: 44)
            }
            .accessibilityLabel("更多想法操作")
            .accessibilityIdentifier("idea-card-menu")
            .padding(.top, 13).padding(.trailing, 13)
        }
    }

    private var createdLabel: String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "zh_CN")
        formatter.dateStyle = .short
        formatter.timeStyle = .short
        formatter.doesRelativeDateFormatting = true
        return formatter.string(from: idea.createdAt)
    }
}

enum IdeaPalette {
    static let background = Color(red: 0.971, green: 0.978, blue: 0.988)
    static let ink = Color(red: 0.06, green: 0.11, blue: 0.20)
    static let secondary = Color(red: 0.40, green: 0.45, blue: 0.54)
    static let faint = Color(red: 0.59, green: 0.63, blue: 0.71)
    static let softGray = Color(red: 0.93, green: 0.95, blue: 0.97)
    static let blue = Color(red: 0.13, green: 0.38, blue: 0.96)
    static let mint = Color(red: 0.54, green: 0.91, blue: 0.72)
    static let green = Color(red: 0.10, green: 0.73, blue: 0.43)
    // Status text on white cards: #008056 gives 4.97:1, above the 4.5:1 normal-text minimum.
    static let positiveText = Color(red: 0, green: 128.0 / 255, blue: 86.0 / 255)
}

struct IdeaPressStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.opacity(enabled ? (configuration.isPressed ? 0.83 : 1) : 0.45)
            .scaleEffect(configuration.isPressed && !reduceMotion ? 0.98 : 1)
    }
}

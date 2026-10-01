import SwiftUI
import NormaCore

struct WorkThreadTerminalView: View {
    let operation: ThreadTaskOutboxOperation
    let agent: String
    let project: String
    let machine: String
    @Binding var keyboardActive: Bool
    @Environment(ComputerModel.self) private var computers
    @Environment(CloudChatModel.self) private var cloud
    @Environment(WorkspaceLibraryModel.self) private var library
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var typeSize
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @State private var inputFocused = false
    @State private var model: WorkThreadTerminalModel
    @State private var retry = 0
    @State private var followingOutput = true
    @State private var showingReconciliation = false
    private let terminalColor = Color(red: 7/255, green: 21/255, blue: 45/255)
    @ScaledMetric(relativeTo: .caption) private var terminalSize = 13.0

    init(operation: ThreadTaskOutboxOperation, agent: String, project: String, machine: String, keyboardActive: Binding<Bool>) {
        self.operation = operation; self.agent = agent; self.project = project; self.machine = machine
        _keyboardActive = keyboardActive
        _model = State(initialValue: WorkThreadTerminalModel(operation: operation))
    }

    var body: some View {
        VStack(spacing: 10) {
            controlBar
            if let error = model.error {
                Text(error).font(.footnote).foregroundStyle(.red).frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("thread-terminal-error")
            }
            terminal
            if model.controlled { keys }
            if let notice = model.notice, !noticeInControlBar {
                Text(notice).font(.caption).foregroundStyle(IdeaPalette.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading).accessibilityIdentifier("thread-terminal-notice")
            }
            if model.canAcknowledge {
                Button("已在电脑核对，继续") { showingReconciliation = true }
                    .frame(minHeight: 44).accessibilityIdentifier("thread-terminal-reconcile")
            }
        }.padding(.horizontal, 16).padding(.bottom, 8)
            .task(id: "\(scenePhase)-\(retry)") {
                guard scenePhase == .active else { model.stop(); return }
                await model.run(library: library, computers: computers, cloud: cloud)
            }
            .onChange(of: scenePhase) { _, value in if value != .active { inputFocused = false; model.stop() } }
            .onChange(of: model.controlled) { _, value in if !value { inputFocused = false } }
            .onChange(of: model.error) { _, value in if value != nil { inputFocused = false } }
            .onChange(of: model.unresolved && !model.working) { _, value in if value { inputFocused = false } }
            .onChange(of: inputFocused) { _, value in keyboardActive = value }
            .onDisappear { inputFocused = false; keyboardActive = false; model.stop() }
            .confirmationDialog("你已核对电脑中的实际输入？", isPresented: $showingReconciliation, titleVisibility: .visible) {
                Button("已核对，保留草稿并继续") { Task { if await model.acknowledge() { retry += 1 } } }
            } message: {
                Text("原操作不会重发。草稿可能已经写入电脑，请核对后再决定输入内容，避免重复执行。")
            }
    }

    private var controlBar: some View {
        let statusLayout = typeSize.isAccessibilitySize ? AnyLayout(HStackLayout(alignment: .firstTextBaseline, spacing: 6))
            : AnyLayout(VStackLayout(alignment: .leading, spacing: 4))
        return HStack(alignment: .center, spacing: 12) {
            statusLayout {
                Group {
                    if typeSize.isAccessibilitySize {
                        Text(model.controlled ? "接管" : model.connected ? "只读" : "未连接")
                    } else {
                        Label(model.controlled ? "已接管 · 自动输入暂停" : model.connected ? "只读 · 原任务终端" : "终端未连接",
                              systemImage: model.controlled ? "circle.fill" : "terminal")
                    }
                }
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(model.controlled ? IdeaPalette.positiveText : IdeaPalette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityLabel(model.controlled ? "已接管，Agent 自动输入暂停，进程仍在运行" : model.connected ? "只读，原任务终端" : "终端未连接")
                    .accessibilityIdentifier("thread-terminal-control-state")
                if noticeInControlBar, let notice = model.notice, notice != "已接管，Agent 自动输入已暂停。" {
                    Text(notice == "已写入终端" ? "已写入" : "已交还")
                        .font(.caption).foregroundStyle(IdeaPalette.secondary).fixedSize(horizontal: false, vertical: true)
                        .accessibilityLabel(notice).accessibilityIdentifier("thread-terminal-notice")
                }
                if !typeSize.isAccessibilitySize {
                    Text(model.controlled ? "进程仍在运行，可直接输入。" : "接管后可使用系统键盘。")
                        .font(.caption).foregroundStyle(IdeaPalette.secondary)
                }
            }.frame(maxWidth: .infinity, alignment: .leading)
            if model.controlled {
                Button { inputFocused = false; model.release() } label: {
                    Group {
                        if typeSize.isAccessibilitySize { Text("交还") }
                        else { Label("交还 \(agent)", systemImage: "arrow.left.arrow.right") }
                    }.font(.subheadline.weight(.semibold)).fixedSize().frame(minHeight: 44)
                }.foregroundStyle(IdeaPalette.blue).disabled(!model.canAct).accessibilityLabel("交还 \(agent)").accessibilityIdentifier("thread-terminal-release")
            } else if model.connected {
                Button(typeSize.isAccessibilitySize ? "接管" : "接管终端") { model.claim() }
                    .font(.subheadline.weight(.semibold)).frame(minHeight: 44)
                    .disabled(!model.canAct || model.latest?.page.running != true).accessibilityLabel("接管终端").accessibilityIdentifier("thread-terminal-claim")
            } else {
                Button("重新连接") { retry += 1 }.frame(minHeight: 44).disabled(model.working)
                    .accessibilityIdentifier("thread-terminal-reconnect")
            }
        }.tint(IdeaPalette.blue)
    }

    private var noticeInControlBar: Bool {
        typeSize.isAccessibilitySize && ["已接管，Agent 自动输入已暂停。", "已写入终端", "已交还 Agent"].contains(model.notice ?? "")
    }

    private var terminal: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 8) {
                ScrollView(.horizontal) {
                    Text("\(agent) · \(machine)").fixedSize()
                        .accessibilityLabel("Agent：\(agent)，电脑：\(machine)")
                }.fixedSize(horizontal: false, vertical: true)
                    .scrollIndicatorsFlash(onAppear: true).accessibilityIdentifier("thread-terminal-identity")
                ScrollView(.horizontal) {
                    Text(project).fixedSize().padding(.horizontal, 8)
                        .accessibilityLabel("项目：\(project)")
                }.fixedSize(horizontal: false, vertical: true).frame(maxWidth: typeSize.isAccessibilitySize ? 100 : 70)
                    .background(.white.opacity(0.1), in: RoundedRectangle(cornerRadius: 6))
                    .scrollIndicatorsFlash(onAppear: true).accessibilityIdentifier("thread-terminal-project")
            }.font(.caption).foregroundStyle(.white.opacity(0.8)).padding(12)
            Divider().overlay(.white.opacity(0.12))
            if let page = model.displayed?.page {
                terminalOutput(page)
                if model.history != nil {
                    Button("回到实时终端") { model.history = nil }.font(.subheadline).foregroundStyle(.white)
                        .frame(maxWidth: .infinity, minHeight: 44).accessibilityIdentifier("thread-terminal-live")
                }
                if model.controlled && model.history == nil {
                    Divider().overlay(.white.opacity(0.12))
                    HStack(spacing: 8) {
                        Image(systemName: "chevron.right").font(.system(size: 16, weight: .medium)).accessibilityHidden(true)
                        TerminalKeyboardField(text: Binding(get: { model.draft?.text ?? "" }, set: { model.updateDraft($0) }),
                            focused: Binding(get: { inputFocused }, set: { inputFocused = $0 }),
                            enabled: model.connected && model.latest?.page.inputReady == true,
                            submit: { model.send(.enter) })
                        if inputFocused {
                            Button { inputFocused = false } label: { Image(systemName: "keyboard.chevron.compact.down").font(.system(size: 20)).frame(width: 44, height: 44) }
                                .accessibilityLabel("收起键盘")
                        }
                    }.foregroundStyle(.white).tint(.white).padding(.horizontal, 12).frame(minHeight: 44)
                }
            } else {
                VStack(spacing: 12) {
                    if model.error == nil { ProgressView().tint(.white) }
                    Text(model.error == nil ? "正在读取原任务终端…" : "原终端暂时不可用").font(.subheadline)
                }.foregroundStyle(.white).frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }.frame(maxWidth: .infinity, minHeight: 196, maxHeight: .infinity)
            .background(terminalColor, in: RoundedRectangle(cornerRadius: 18))
            .clipShape(RoundedRectangle(cornerRadius: 18))
    }

    private func terminalOutput(_ page: TerminalPage) -> some View {
                GeometryReader { viewport in
                    ScrollViewReader { proxy in
                        scrollingGrid(page)
                            .onPreferenceChange(TerminalBottomPosition.self) { value in
                                followingOutput = value <= viewport.size.height + 24
                            }
                            .onAppear { revealLatest(using: proxy) }
                            .onChange(of: inputFocused) { _, focused in
                                if focused { revealLatest(using: proxy) }
                            }
                            .onChange(of: viewport.size) { _, _ in
                                if inputFocused || followingOutput { revealLatest(using: proxy) }
                            }
                            .onChange(of: model.history?.page.startLine) { _, value in
                                if value == nil { revealLatest(using: proxy) }
                                else { proxy.scrollTo("terminal-top", anchor: .topLeading) }
                            }
                            .onChange(of: model.latest?.page.revision) { _, _ in
                                if model.history == nil && (inputFocused || followingOutput) {
                                    revealLatest(using: proxy)
                                }
                            }
                    }
                }.accessibilityElement(children: .contain).accessibilityIdentifier("thread-terminal-viewport")
    }

    private func revealLatest(using proxy: ScrollViewProxy) {
        if model.history == nil && model.latest?.page.cursor != nil {
            proxy.scrollTo("terminal-cursor", anchor: .bottomTrailing)
        } else { proxy.scrollTo("terminal-bottom", anchor: .bottomLeading) }
    }

    private func scrollingGrid(_ page: TerminalPage) -> some View {
        ScrollView([.horizontal, .vertical]) {
                            VStack(alignment: .leading, spacing: 0) {
                                Color.clear.frame(height: 1).id("terminal-top")
                                if page.startLine > 0 {
                                    Button("更早的输出") { inputFocused = false; model.loadOlder() }
                                        .font(.caption).foregroundStyle(.white).frame(minHeight: 44)
                                        .disabled(!model.connected || model.working)
                                }
                                terminalRows(page)
                                Color.clear.frame(height: 1).id("terminal-bottom")
                                    .background(GeometryReader { bottom in
                                        Color.clear.preference(key: TerminalBottomPosition.self,
                                            value: bottom.frame(in: .named("thread-terminal-scroll")).maxY)
                                    })
                            }.padding(.vertical, 12)
                        }.padding(.horizontal, 12).coordinateSpace(name: "thread-terminal-scroll")
    }

    private func terminalRows(_ page: TerminalPage) -> some View {
        // Preserve the Mac's grid columns. Horizontal scrolling avoids inventing cursor positions by reflowing.
        let font = UIFont.monospacedSystemFont(ofSize: terminalSize, weight: .regular)
        let cell = ("M" as NSString).size(withAttributes: [.font: font]).width
        let line = ceil(font.lineHeight) + 3
        return ZStack(alignment: .topLeading) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(page.rows.indices, id: \.self) { index in
                    Text(verbatim: page.rows[index].isEmpty ? " " : page.rows[index])
                        .font(Font(font)).foregroundStyle(.white.opacity(0.92))
                        .frame(height: line, alignment: .leading).fixedSize(horizontal: true, vertical: false)
                }
            }.frame(width: cell * CGFloat(page.cols), alignment: .leading)
                .accessibilityElement(children: .ignore).accessibilityLabel(page.text())
                .accessibilityIdentifier("thread-terminal-output")
            if model.history == nil, let cursor = page.cursor {
                // Use layout coordinates: a drawing offset leaves ScrollViewReader's target at the grid origin.
                VStack(alignment: .leading, spacing: 0) {
                    Color.clear.frame(height: line * CGFloat(cursor.line - page.startLine))
                    HStack(spacing: 0) {
                        Color.clear.frame(width: cell * CGFloat(cursor.column), height: font.lineHeight)
                        VStack(spacing: 0) {
                            Rectangle().fill(.white.opacity(0.8)).frame(width: cell, height: font.lineHeight)
                                .accessibilityElement().accessibilityLabel("终端光标，第 \(cursor.line + 1) 行，第 \(cursor.column + 1) 列")
                                .accessibilityIdentifier("thread-terminal-cursor")
                            Color.clear.frame(height: 8)
                        }.padding(.trailing, 8).id("terminal-cursor")
                    }
                }.fixedSize().allowsHitTesting(false)
            }
        }
    }

    private var keys: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                terminalKey("Esc", input: .escape)
                terminalKey("Tab", input: .tab)
                terminalKey("Ctrl-C", input: .ctrlC)
                terminalKey("↑", input: .up, label: "方向键上")
                terminalKey("↓", input: .down, label: "方向键下")
                terminalKey("←", input: .left, label: "方向键左")
                terminalKey("→", input: .right, label: "方向键右")
                terminalKey("⌫", input: .backspace, label: "终端退格")
            }.padding(4)
        }.background {
            let shape = RoundedRectangle(cornerRadius: 12)
            if reduceTransparency { shape.fill(IdeaPalette.background) }
            else if #available(iOS 26.0, *) { Color.clear.glassEffect(.regular, in: shape) }
            else { shape.fill(.regularMaterial) }
        }
    }
    private func terminalKey(_ title: String, input: TerminalInput, label: String? = nil) -> some View {
        Button { model.send(input) } label: {
            Text(title).font(.subheadline.monospaced()).frame(minWidth: 52, minHeight: 44)
                .background(IdeaPalette.faint.opacity(0.18), in: RoundedRectangle(cornerRadius: 8))
        }.disabled(!model.canInput).accessibilityLabel(label ?? title).tint(IdeaPalette.ink)
    }
}

private struct TerminalBottomPosition: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

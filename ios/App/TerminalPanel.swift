import SwiftUI
import NormaCore

struct TerminalPanel: View {
    let model: ComputerModel
    let machine: PairedMachine
    let workspaceID: String
    let routeID: String
    let card: CardSnapshot
    let project: ProjectSnapshot?
    let revision: UInt64
    let canControl: Bool
    @Environment(\.scenePhase) private var scenePhase
    @FocusState private var inputFocused: Bool
    @State private var page: TerminalPage?
    @State private var history: TerminalPage?
    @State private var connected = false
    @State private var checkedAt: Date?
    @State private var text = ""
    @State private var notice: String?
    @State private var failure: String?
    @State private var busy = false
    @State private var readingHistory = false
    @State private var retry = 0
    @State private var work: Task<Void, Never>?
    @State private var showResult = false
    private var displayed: TerminalPage? { history ?? page }
    private var pending: PendingMobileCommand? {
        model.pendingCommands.first {
            guard $0.machineID == machine.id, $0.request.workspaceID == workspaceID, $0.request.routeID == routeID,
                  case let .terminalInput(target, _, _, _) = $0.request.command else { return false }
            return target.id == card.id
        }
    }
    private var canType: Bool {
        canControl && connected && scenePhase == .active && history == nil && page?.inputReady == true && !busy && pending == nil
    }
    private var scope: TerminalRequest? {
        guard let run = card.runID else { return nil }
        return TerminalRequest(workspaceID: workspaceID, routeID: routeID, nodeID: card.id, runID: run, sessionID: card.sessionID)
    }
    private let keys: [TerminalInput] = [.enter, .tab, .escape, .up, .down, .left, .right, .backspace, .ctrlC]

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Label(history != nil ? "历史回看" : connected ? "实时终端" : failure == nil ? "连接中" : "已断开", systemImage: history != nil ? "clock" : "terminal")
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                if !connected, let checkedAt { Text(checkedAt, style: .time).font(.caption2).foregroundStyle(.secondary) }
                if history != nil {
                    Button("回到实时") { history = nil; failure = nil }.font(.caption).frame(minHeight: 44)
                }
                Button { retry += 1 } label: { Image(systemName: "arrow.clockwise") }
                    .frame(width: 44, height: 44).accessibilityLabel("重新连接终端")
            }.padding(.horizontal, 16)
            if let displayed {
                ScrollView([.vertical, .horizontal]) {
                    VStack(alignment: .leading, spacing: 8) {
                        if displayed.startLine > 0 {
                            Button(readingHistory ? "读取中…" : "更早的输出") { work = Task { await loadOlder() } }
                                .font(.caption).frame(minHeight: 44).disabled(readingHistory || !connected)
                                .accessibilityIdentifier("terminal-history")
                        }
                        Text(displayed.rows.joined(separator: "\n"))
                            .font(.system(.caption, design: .monospaced)).foregroundStyle(.white.opacity(0.9))
                            .textSelection(.enabled).fixedSize(horizontal: true, vertical: true)
                            .accessibilityIdentifier("terminal-output")
                    }.padding(.horizontal, 16).padding(.bottom, 16)
                }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            } else if scope == nil {
                ContentUnavailableView("终端尚未启动", systemImage: "terminal", description: Text("在电脑上启动这张 Agent 卡片后即可查看。"))
            } else if failure == nil {
                ProgressView("正在读取终端…").frame(maxWidth: .infinity, maxHeight: .infinity)
            } else { Spacer() }
            if let failure { Text(failure).font(.caption).foregroundStyle(.orange).frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.vertical, 6) }
            if let notice { Text(notice).font(.caption).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16) }
        }
        .background(.black).scrollDismissesKeyboard(.interactively)
        .safeAreaInset(edge: .bottom) { composer }
        .toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                Spacer()
                Button("收起键盘", systemImage: "keyboard.chevron.compact.down") { inputFocused = false }
                    .labelStyle(.iconOnly).accessibilityIdentifier("terminal-dismiss-keyboard")
            }
        }
        .task(id: "\(scenePhase)-\(card.runID ?? "none")-\(card.sessionID ?? "none")-\(retry)") {
            connected = false; history = nil; failure = nil
            guard scenePhase == .active, let scope else { return }
            do {
                for try await update in model.client.terminalUpdates(machine, request: scope) {
                    try Task.checkCancellation()
                    switch update {
                    case let .current(next): page = next; checkedAt = .now; connected = true; failure = nil
                    case .heartbeat: break
                    case .recovering: connected = false; failure = "连接中断，显示最后一次内容…"
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                connected = false; failure = error.localizedDescription
            }
        }
        .sheet(isPresented: $showResult) {
            if let project, let pending { CardCommandSheet(model: model, machine: machine, project: project, revision: revision, pending: pending) }
        }
        .onDisappear { connected = false; work?.cancel() }
    }

    private var composer: some View {
        VStack(alignment: .leading, spacing: 8) {
            if pending != nil {
                Button("查询上次输入结果") { showResult = true }.font(.subheadline).frame(minHeight: 44)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(keys.indices, id: \.self) { index in
                        Button(keys[index].label) { work = Task { await send(keys[index]) } }
                            .font(.caption.monospaced().weight(.medium)).frame(minWidth: 44, minHeight: 44)
                            .background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
                            .accessibilityLabel(keyLabel(keys[index])).accessibilityIdentifier("terminal-key-\(index)")
                    }
                }.disabled(!canType)
            }
            HStack(alignment: .bottom, spacing: 10) {
                TextField("输入到电脑终端", text: $text, axis: .vertical)
                    .focused($inputFocused)
                    .font(.subheadline).lineLimit(1...4).autocorrectionDisabled().textInputAutocapitalization(.never)
                    .padding(12).background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 14))
                    .accessibilityIdentifier("terminal-input")
                Button { work = Task { await send(.text(text)) } } label: {
                    if busy { ProgressView() } else { Image(systemName: "arrow.up") }
                }.frame(width: 44, height: 44).background(.white.opacity(0.12), in: Circle())
                    .disabled(!canType || text.isEmpty || text.utf8.count > 4096)
                    .accessibilityLabel("填入终端").accessibilityIdentifier("terminal-send")
            }
            Text(history != nil ? "回到实时画面后可输入" : !canControl ? "此项目为只读权限" : "文字先填入终端，点 Enter 确认")
                .font(.caption2).foregroundStyle(.secondary)
        }.padding(.horizontal, 16).padding(.vertical, 10).background(.ultraThinMaterial)
    }
    private func keyLabel(_ key: TerminalInput) -> String {
        switch key { case .up: "方向键上"; case .down: "方向键下"; case .left: "方向键左"; case .right: "方向键右"; default: key.label }
    }
    @MainActor private func send(_ input: TerminalInput) async {
        guard canType, let page else { return }
        busy = true; failure = nil; notice = nil
        defer { busy = false }
        do {
            let command = try await model.client.prepareTerminalInput(machine, workspaceID: workspaceID, routeID: routeID, revision: revision, page: page, input: input)
            try await model.commandStore.save(command); try await model.reloadCommands()
            var result = try await model.client.submit(command, to: machine)
            //20×100ms=2s for the existing durable receipt; later recovery only queries the saved operation.
            for _ in 0..<20 where result.state == "accepted" {
                try await Task.sleep(for: .milliseconds(100))
                result = try await model.client.receipt(for: command, on: machine)
            }
            if result.state == "succeeded" {
                notice = "已写入终端"
                if case let .text(submitted) = input, text == submitted { text = "" }
                try await model.commandStore.remove(id: command.id); try await model.reloadCommands()
            } else if result.state == "rejected" {
                failure = result.error == "terminal_changed" ? MobileError.terminalChanged.localizedDescription
                    : result.error == "paste_unavailable" ? "终端暂不支持多行粘贴，请分行输入。" : "输入未发送，请查看最新终端后重试。"
                try await model.commandStore.remove(id: command.id); try await model.reloadCommands()
            } else { notice = "输入结果尚未确认，请查询上次结果。" }
        } catch {
            if !Task.isCancelled { failure = error.localizedDescription }
        }
    }
    @MainActor private func loadOlder() async {
        guard var scope, let displayed, displayed.startLine > 0 else { return }
        readingHistory = true
        defer { readingHistory = false }
        scope.before = displayed.startLine; scope.revision = displayed.revision
        do {
            let older = try await model.client.terminal(machine, request: scope)
            try Task.checkCancellation(); history = older; failure = nil
        } catch { if !Task.isCancelled { failure = error.localizedDescription } }
    }
}

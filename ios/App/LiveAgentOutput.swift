import SwiftUI
import NormaCore

/// Native agents write history after a message completes. Show their actual screen meanwhile.
struct LiveAgentOutput: View {
    let client: MobileClient
    let machine: PairedMachine
    let request: TerminalRequest
    let profile: String
    let advanced: () -> Void
    @Environment(\.scenePhase) private var scenePhase
    @State private var output = ""
    @State private var failure: String?
    @State private var received = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 7) {
                ProviderMark(profile: profile, size: 18, working: failure == nil)
                Text("\(profile) · 实时输出")
                    .font(.caption.weight(.medium)).foregroundStyle(WorkspaceStyle.muted)
            }
            if !output.isEmpty {
                Text(output).font(.system(.caption, design: .monospaced)).lineSpacing(4)
                    .textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("chat-live-output")
            } else if failure == nil {
                if received { Text("等待 Agent 输出…").font(.caption).foregroundStyle(.secondary) }
                else { WorkspaceLoadingView(message: "正在连接实时输出…", inline: true) }
            }
            if let failure { Text(failure).font(.caption).foregroundStyle(.secondary) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14).background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
        .task(id: "\(scenePhase)-\(request.runID)-\(request.sessionID ?? "none")") {
            output = ""; failure = nil; received = false
            guard scenePhase == .active else { return }
            do {
                for try await update in client.terminalUpdates(machine, request: request) {
                    try Task.checkCancellation()
                    switch update {
                    case let .current(page):
                        received = true
                        // Preview the most recent screen; the Terminal tab retains full paged output.
                        let text = page.text(lastRows: 24).trimmingCharacters(in: .whitespacesAndNewlines)
                        if text != output { output = text; advanced() }
                        failure = nil
                    case .heartbeat: break
                    case .recovering: failure = "正在重连，显示最后收到的输出…"
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                failure = (error as? MobileError) == .terminalUnavailable
                    ? "请更新电脑端 Norma 以查看实时输出。完整回复仍会同步到对话。"
                    : error.localizedDescription
            }
        }
    }
}

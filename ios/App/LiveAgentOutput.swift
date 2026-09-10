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

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("\(profile) · 实时输出", systemImage: "waveform")
                .font(.caption.weight(.medium)).foregroundStyle(.secondary)
            if !output.isEmpty {
                Text(output).font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("chat-live-output")
            } else if failure == nil { ProgressView("正在连接实时输出…").font(.caption) }
            if let failure { Text(failure).font(.caption).foregroundStyle(.secondary) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14).background(.white.opacity(0.04), in: RoundedRectangle(cornerRadius: 16))
        .task(id: "\(scenePhase)-\(request.runID)-\(request.sessionID ?? "none")") {
            output = ""; failure = nil
            guard scenePhase == .active else { return }
            do {
                for try await update in client.terminalUpdates(machine, request: request) {
                    try Task.checkCancellation()
                    switch update {
                    case let .current(page):
                        // Preview the most recent screen; the Terminal tab retains full paged output.
                        let text = page.rows.suffix(24).joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
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

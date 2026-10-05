import SwiftUI
import NormaCore

@MainActor @Observable final class WorkspaceTerminalModel {
    private(set) var frame: WorkspaceTerminalFrame?
    private(set) var history: WorkspaceTerminalFrame?
    private(set) var connected = false
    private(set) var loadingHistory = false
    private(set) var failure: String?
    private(set) var checkedAt: Date?
    private var client: WorkspaceTerminalClient?
    private var scope: WorkspaceTerminalRequest?
    private var identity: WorkspaceAgentTarget?
    private var historyWork: Task<Void, Never>?
    private var generation = UUID()
    var displayed: WorkspaceTerminalFrame? { history ?? frame }

    func watch(_ client: WorkspaceTerminalClient, request: WorkspaceTerminalRequest) async {
        let ownGeneration = UUID(); generation = ownGeneration
        self.client = client; scope = request
        if identity != request.target { frame = nil; history = nil; identity = request.target }
        failure = nil; connected = false
        do {
            while !Task.isCancelled && generation == ownGeneration {
                do {
                    if !loadingHistory {
                        var scope = request; scope.knownVersion = frame?.version
                        if let next = try await client.frame(scope) {
                            try Task.checkCancellation()
                            guard generation == ownGeneration else { break }
                            frame = next
                        }
                        connected = true; failure = nil; checkedAt = .now
                    }
                } catch {
                    try Task.checkCancellation()
                    guard generation == ownGeneration else { break }
                    connected = false
                    let retryable: [MobileError] = [.connectionFailed, .timedOut, .unavailable, .busy]
                    let canRetry = (error as? MobileError).map { retryable.contains($0) } == true || error as? WorkspaceChatError == .unavailable
                    guard canRetry else { throw error }
                    failure = "连接中断，正在重连；显示最后收到的画面。"
                    await client.close()
                    try await Task.sleep(for: .seconds(2))
                }
                try await Task.sleep(for: await client.refreshDelay)
            }
        } catch {
            if !Task.isCancelled && generation == ownGeneration { failure = message(error); connected = false }
        }
        await client.close()
        if generation == ownGeneration { connected = false; self.client = nil }
    }
    func stop() { generation = UUID(); historyWork?.cancel(); historyWork = nil; loadingHistory = false; connected = false; client = nil }
    func pauseReading() { if history == nil { history = frame } }
    func returnToLive() {
        historyWork?.cancel(); historyWork = nil; loadingHistory = false; history = nil
        // History navigation may clear its recoverable error, but a stopped session/grant notice remains.
        if connected { failure = nil }
    }
    func loadOlder() {
        guard !loadingHistory, connected, let client, var request = scope, let current = displayed, current.startLine > 0 else { return }
        loadingHistory = true
        request.before = current.startLine; request.historyVersion = current.version
        let ownGeneration = generation
        historyWork = Task {
            defer { if generation == ownGeneration { loadingHistory = false } }
            do {
                if let next = try await client.frame(request) {
                    try Task.checkCancellation()
                    guard generation == ownGeneration else { return }
                    history = next; failure = nil
                }
            } catch { if !Task.isCancelled && generation == ownGeneration { failure = message(error) } }
        }
    }
    private func message(_ error: any Error) -> String {
        switch error as? MobileError {
        case .sessionChanged: "原 Agent 会话已变化，无法查看该会话；结果仍可阅读。"
        case .terminalUnavailable: "这个 Agent 当前没有可查看的终端；请切换到结果。"
        case .unsupportedVersion: "请更新 Mac 上的 Norma，以查看实时终端。"
        case .terminalChanged: "历史画面已变化，请回到实时后重新读取。"
        case .unauthorized, .grantChanged: "读取授权已变化，请在 Mac 检查权限。"
        default: error.localizedDescription
        }
    }
}

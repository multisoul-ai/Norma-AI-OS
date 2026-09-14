import Foundation
import NormaCore

extension VoiceCall {
    func startMacBinding() {
        macBindingTask?.cancel()
        guard let callID, let target, provider == .openai || doubaoTools else {
            toolsStatus = "未绑定 Mac 项目，可以继续聊天"
            return
        }
        let current = generation
        toolsStatus = "正在连接本次通话的 Mac 项目"
        macBindingTask = Task {
            do {
                let client: RealtimeBindingClient
                if let cached = macBindingClient { client = cached }
                else { client = try await mobileClient.prepareRealtimeBinding(target.machine) }
                guard current == generation, wantsCall, !Task.isCancelled else { return }
                macBindingClient = client
                // Three reconnects at 1 + 2 + 4 = 7 seconds. Only binding is retried; no tool command.
                for attempt in 0...3 {
                    guard current == generation, wantsCall, !Task.isCancelled else { return }
                    bindingGeneration += 1
                    let binding = try RealtimeBinding(callID: callID, workspaceID: target.project.id, generation: bindingGeneration)
                    do {
                        let gateway = provider == .doubao ? try await requestGatewayTicket(callID: callID) : nil
                        for try await update in client.updates(binding: binding, gateway: gateway) {
                            guard current == generation, wantsCall, !Task.isCancelled else { return }
                            if let observed = update.snapshotObservedAt { snapshotObservedAt = observed }
                            switch update.state {
                            case .connecting: toolsStatus = "正在连接本次通话的 Mac 项目"
                            case .attached: toolsStatus = "Mac 已连接，任务工具尚未就绪"
                            case .ready: toolsStatus = "任务将安排到 \(target.project.displayName)"
                            case .unavailable: macUnavailable(bindingError(update.error))
                            }
                        }
                        return
                    } catch {
                        guard current == generation, wantsCall, !Task.isCancelled else { return }
                        let retryable = [MobileError.connectionFailed, .timedOut, .unavailable]
                        guard attempt < 3, (error as? MobileError).map(retryable.contains) ?? (error is URLError) else { throw error }
                        macUnavailable("Mac 连接中断，正在核对绑定；可以继续聊天")
                        try await Task.sleep(for: .seconds(1 << attempt))
                    }
                }
            } catch {
                guard current == generation, wantsCall, !Task.isCancelled else { return }
                macUnavailable("Mac 工具不可用，可以继续聊天。" + error.localizedDescription)
            }
        }
    }

    private func requestGatewayTicket(callID: String) async throws -> RealtimeGatewayTicket {
        guard doubaoTools, let sender = doubaoSender else { throw MobileError.unavailable }
        let requestID = UUID().uuidString, current = generation
        gatewayRequestID = requestID; gatewayTicket = nil
        defer {
            if gatewayRequestID == requestID { gatewayRequestID = nil; gatewayTicket = nil }
        }
        sender.send(["type":"gateway.tools.request", "request_id":requestID])
        // 60 × 50ms = 3s to obtain a fresh one-use ticket before each Mac binding attempt.
        for _ in 0..<60 {
            try Task.checkCancellation()
            guard generation == current, self.callID == callID else { throw CancellationError() }
            if let ticket = gatewayTicket { return ticket }
            try await Task.sleep(for: .milliseconds(50))
        }
        throw MobileError.timedOut
    }

    private func macUnavailable(_ message: String) {
        toolsStatus = message
        if let update = localConfiguration.offlineUpdate { try? realtime?.send(update) }
    }

    private func bindingError(_ code: String?) -> String {
        switch code {
        case "official_openai_required": "Mac 需配置同一 OpenAI 项目的官方服务，当前可以继续聊天"
        case "credential_unavailable": "Mac 的 OpenAI 凭据不可用，当前可以继续聊天"
        case "sideband_authentication_failed": "Mac 无法加入本次通话，请核对两端的 OpenAI 项目；可以继续聊天"
        default: "Mac 工具连接中断，可以继续聊天；任务状态请查看记录"
        }
    }
}

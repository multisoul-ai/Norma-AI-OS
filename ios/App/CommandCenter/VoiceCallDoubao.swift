import Foundation
import NormaCore

extension VoiceCall {
    func startDoubao(ticket: VoiceSessionTicket, current: Int) {
        var request = URLRequest(url: ticket.url, timeoutInterval: 12)
        request.setValue("Bearer \(ticket.token)", forHTTPHeaderField: "Authorization")
        let session = URLSession(configuration: .ephemeral, delegate: VoiceGatewayRedirects(), delegateQueue: nil)
        let ws = session.webSocketTask(with: request)
        voiceSession = session; socket = ws
        // 512 KiB accommodates six seconds of PCM16/24k base64 plus metadata; playback has its own budget.
        ws.maximumMessageSize = 512 * 1024
        doubaoSender = DoubaoSender(socket: ws, muted: muted) { [weak self] in
            Task { @MainActor in
                guard let self, self.generation == current else { return }
                self.suspend("音频发送中断或积压，请重新接通")
            }
        }
        audio.onError = { [weak self] message in
            Task { @MainActor in
                guard let self, self.generation == current else { return }
                self.suspend(message)
            }
        }
        ws.resume()
        connectDeadline = Task {
            // 12s transport request + 3s for the first real microphone callback.
            try? await Task.sleep(for: .seconds(15))
            if !Task.isCancelled, current == generation, state != .connected { lostConnection() }
        }
        receiveTask = Task { [weak self] in
            do {
                while !Task.isCancelled {
                    let message = try await ws.receive()
                    let data: Data
                    switch message {
                    case .string(let text): data = Data(text.utf8)
                    case .data(let bytes): data = bytes
                    @unknown default: throw CallError.message("无法识别语音消息")
                    }
                    guard let event = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                        throw CallError.message("语音消息格式错误")
                    }
                    if event["type"] as? String == "session.closed" {
                        if let self, generation == current { hangup() }
                        return // Teardown also observes this ACK after the UI generation is detached.
                    }
                    guard let self else { return }
                    guard generation == current else { continue }
                    try await handleDoubao(event, current: current)
                }
            } catch {
                guard let self, generation == current else { return }
                lostConnection()
            }
        }
        status = "正在连接豆包"
        toolsStatus = "豆包语音已选择，Mac 任务工具尚未接入"
    }

    func handleDoubao(_ event: [String: Any], current: Int) async throws {
        guard current == generation else { return }
        let id = event["response_id"] as? String ?? ""
        switch event["type"] as? String ?? "" {
        case "session.created":
            guard state == .connecting || state == .reconnecting, let sender = doubaoSender else { return }
            audio.onFrame = { frame in sender.append(frame) }
            try await audio.start(speaker: speaker, owner: current)
            guard current == generation, wantsCall else { await audio.stop(owner: current); return }
            connectDeadline?.cancel(); state = .connected; status = "我在听"; phase = .listening
            connectedAt = ProcessInfo.processInfo.systemUptime; updateRoute(); audio.setMuted(muted)
            if muted { sender.setMuted(true) }
            configureDoubaoMedia(event)
            if doubaoTools { startMacBinding() }
            if doubaoReports, let target, let client = macBindingClient {
                startReportObservation(client: client, target: target)
            }
        case "gateway.tools.ticket":
            guard let requestID = gatewayRequestID, event["request_id"] as? String == requestID,
                  let endpoint = voiceGatewayEndpoint, let callID else { return }
            gatewayTicket = try RealtimeGatewayTicket(data: JSONSerialization.data(withJSONObject: event),
                endpoint: endpoint, callID: callID)
        case "gateway.report.deferred":
            guard event["report_id"] as? String == reportBatch?.id else { return }
            interruptReport()
            // A busy gateway must not cause the 30Hz UI tick to flood report requests.
            reportRetryAfter = ProcessInfo.processInfo.systemUptime + 1
        case "conversation.item.input_audio_transcription.started":
            beginDiscussion(event)
            userSpeaking = true
            await interruptDoubao().value
            guard current == generation else { return }
            phase = .listening; status = "我在听"; userSubtitle = ""
        case "conversation.item.input_audio_transcription.delta":
            userSubtitle = String((event["transcript"] as? String ?? (userSubtitle + (event["delta"] as? String ?? ""))).suffix(300))
        case "conversation.item.input_audio_transcription.completed":
            recordDiscussion(event)
            userSpeaking = false
            userSubtitle = String((event["transcript"] as? String ?? userSubtitle).suffix(300))
            phase = .thinking; status = "想一想"
        case "conversation.item.input_audio_transcription.failed":
            userSpeaking = false; phase = .listening; status = "刚才没听清，可以再说一次"
        case "response.output_text.delta", "response.output_text.done", "response.output_audio.started", "response.output_audio.delta":
            guard !id.isEmpty, id.utf8.count <= 128, !ignored.contains(id) else { return }
            if let reportID = event["report_id"] as? String {
                guard event["type"] as? String == "response.output_audio.started",
                      reportBatch?.id == reportID, !reportsPaused, !userSpeaking,
                      activeResponse.isEmpty || activeResponse == id else {
                    if ignored.count >= 64 { ignored.removeAll() }
                    ignored.insert(id)
                    doubaoSender?.send(["type":"gateway.interrupt", "response_id":id, "played_ms":0])
                    return
                }
            }
            guard !userSpeaking else { return }
            if id != activeResponse {
                if event["report_id"] as? String != nil {
                    reportResponse = id; waitingFirstAudio = true; outputEnded = false
                } else { _ = stopPlayback() }
                activeResponse = id; subtitle = ""
                await audio.interrupt()
            }
            guard current == generation, id == activeResponse, !userSpeaking else { return }
            switch event["type"] as? String {
            case "response.output_text.delta": subtitle = String((subtitle + (event["delta"] as? String ?? "")).suffix(600))
            case "response.output_text.done": subtitle = String((event["text"] as? String ?? subtitle).suffix(600))
            case "response.output_audio.delta":
                guard let encoded = event["delta"] as? String, let pcm = Data(base64Encoded: encoded),
                      !pcm.isEmpty, pcm.count.isMultiple(of: 2), pcm.count <= 288_000 else {
                    suspend("收到的语音数据无效，请重新接通"); return
                }
                // 24k × 2 bytes × 6s = 288000 bytes, matching the total playback queue limit.
                if id == reportResponse, waitingFirstAudio {
                    boundReportDuration(); waitingFirstAudio = false
                }
                audio.enqueue(pcm); outputEnded = false; phase = .speaking; status = "正在说话"
            default: break
            }
        case "response.output_audio.done":
            if !id.isEmpty, id == activeResponse, !ignored.contains(id) { outputEnded = true }
        case "gateway.behavior":
            if !id.isEmpty, id == activeResponse, !ignored.contains(id) { _ = applyBehavior(event) }
        case "response.function_call_arguments.done":
            guard !id.isEmpty, id == activeResponse, !ignored.contains(id), !userSpeaking else { return }
            let items = event["items"] as? [[String: Any]] ?? []
            guard items.count <= 8 else { suspend("角色动作请求过多，请重新接通"); return }
            let results: [[String: Any]] = items.compactMap { item in
                guard item["name"] as? String == "set_behavior", let callID = item["call_id"] as? String,
                      !callID.isEmpty, callID.utf8.count <= 128 else { return nil }
                let applied = applyBehavior(item)
                return ["call_id": callID, "role": "tool", "content": [["type": "input_text",
                    "text": applied ? "{\"applied\":true}" : "{\"applied\":false}"]]]
            }
            if !results.isEmpty { doubaoSender?.send(["type": "conversation.item.create", "items": results]) }
        case "gateway.error":
            let error = event["error"] as? [String: Any]
            if ["context_truncation_unavailable", "context_sync_timeout", "context_sync_race"].contains(error?["code"] as? String ?? "") {
                suspend("插话后的对话记录未能同步，请重新接通")
            } else if error?["retryable"] as? Bool == true { lostConnection() }
            else { suspend("豆包语音服务中断，请检查服务端配置后重新接通") }
        case "session.closed": hangup()
        default: break
        }
    }

    @discardableResult func interruptDoubao() -> Task<Void, Never> {
        let id = activeResponse, current = generation, sender = doubaoSender
        _ = stopPlayback(); subtitle = ""; phase = .listening; status = "我在听"
        return Task {
            let played = await audio.interrupt()
            guard current == generation, !id.isEmpty else { return }
            sender?.send(["type": "gateway.interrupt", "response_id": id, "played_ms": played])
        }
    }

    func closeDoubao() {
        guard let old = socket else { return }
        let oldSession = voiceSession, receiver = receiveTask, id = activeResponse
        socket = nil; voiceSession = nil; receiveTask = nil
        doubaoSender?.close(); doubaoSender = nil; audio.onFrame = nil; audio.onError = nil
        closingTask = Task {
            let played = await audio.interrupt()
            await audio.stop()
            // ACK preferred; a lost peer cannot retain the paid slot locally for more than two seconds.
            let deadline = Task {
                try? await Task.sleep(for: .seconds(2))
                if !Task.isCancelled { old.cancel(with: .goingAway, reason: nil) }
            }
            if !id.isEmpty, let data = try? JSONSerialization.data(withJSONObject: ["type": "gateway.interrupt", "response_id": id, "played_ms": played]) {
                try? await old.send(.string(String(decoding: data, as: UTF8.self)))
            }
            try? await old.send(.string("{\"type\":\"session.close\"}"))
            await receiver?.value
            deadline.cancel(); old.cancel(with: .normalClosure, reason: nil)
            receiver?.cancel(); oldSession?.invalidateAndCancel()
        }
    }
}

/// Credentials and one-use tickets must not follow gateway redirects to another endpoint.
final class VoiceGatewayRedirects: NSObject, URLSessionTaskDelegate, Sendable {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

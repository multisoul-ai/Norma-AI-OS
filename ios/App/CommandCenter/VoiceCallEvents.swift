import Foundation

extension VoiceCall {
    func handleOpenAI(_ event: [String: Any], current: Int) throws {
        guard let rtc = realtime else { return }
        history.observe(event)
        let type = event["type"] as? String ?? ""
        let id = event["response_id"] as? String ?? ""
        switch type {
        case "session.created":
            if let session = event["session"] as? [String: Any] { localConfiguration.capture(session) }
            guard captureTask == nil else { return }
            let replay = history.replayEvents
            pendingHistory = Set(replay.compactMap { ($0["item"] as? [String: Any])?["id"] as? String })
            for event in replay { try rtc.send(event) }
            captureTask = Task {
                // 30 × 100 ms = 3 seconds to prove actual hardware callbacks, before unmuting.
                for _ in 0..<30 {
                    guard current == generation, wantsCall, !Task.isCancelled else { return }
                    if rtc.audio.snapshot().captured > 0, pendingHistory.isEmpty {
                        connectDeadline?.cancel(); rtc.setMuted(muted)
                        state = .connected; status = "我在听"; phase = .listening
                        connectedAt = ProcessInfo.processInfo.systemUptime
                        lastActivity = ProcessInfo.processInfo.systemUptime; updateRoute()
                        return
                    }
                    try? await Task.sleep(for: .milliseconds(100))
                }
                if current == generation { suspend("音频或会话尚未就绪，请重新接通") }
            }
        case "conversation.item.added":
            if let item = event["item"] as? [String: Any], let id = item["id"] as? String { pendingHistory.remove(id) }
        case "input_audio_buffer.speech_started":
            beginDiscussion(event)
            userSpeaking = true; _ = stopPlayback() // Server VAD cancels and truncates its WebRTC buffer.
            phase = .listening; status = "我在听"; userSubtitle = ""; lastActivity = ProcessInfo.processInfo.systemUptime
        case "input_audio_buffer.speech_stopped":
            userSpeaking = false; phase = .thinking; status = "想一想"
            speechEndedAt = ProcessInfo.processInfo.systemUptime; waitingFirstAudio = true
            lastActivity = ProcessInfo.processInfo.systemUptime
        case "conversation.item.input_audio_transcription.completed":
            recordDiscussion(event)
            userSubtitle = String((event["transcript"] as? String ?? "").suffix(300))
        case "conversation.item.input_audio_transcription.failed":
            status = "字幕暂时不可用，语音通话仍可继续"
        case "response.created":
            if let response = event["response"] as? [String: Any], let responseID = response["id"] as? String {
                if let metadata = response["metadata"] as? [String: String], let reportID = metadata["norma_report"],
                   reportID != reportBatch?.id || userSpeaking || reportsPaused {
                    ignored.insert(responseID)
                    try rtc.send(["type":"response.cancel", "response_id":responseID])
                    return // A late out-of-band response must not revive an interrupted report.
                }
                activeResponse = responseID; subtitle = ""; playedAfter = nil
                if let metadata = response["metadata"] as? [String: String], metadata["norma_report"] == reportBatch?.id {
                    reportResponse = responseID
                } else if reportBatch != nil { interruptReport() }
                phase = .thinking
            }
        case "response.output_audio_transcript.delta":
            guard !ignored.contains(id), id == activeResponse else { return }
            subtitle = String((subtitle + (event["delta"] as? String ?? "")).suffix(600))
        case "response.output_audio_transcript.done":
            if !ignored.contains(id), id == activeResponse { subtitle = String((event["transcript"] as? String ?? subtitle).suffix(600)) }
        case "output_audio_buffer.started":
            guard !ignored.contains(id), id == activeResponse, !userSpeaking else { return }
            rtc.audio.silenceOutput(false); phase = .speaking; status = "正在说话"; playedAfter = nil
            if id == reportResponse { boundReportDuration() }
        case "output_audio_buffer.stopped":
            if !ignored.contains(id), id == activeResponse {
                // Leave time for native jitter/device buffers, then require a quiet render sample.
                playedAfter = ProcessInfo.processInfo.systemUptime + max(0.3, rtc.audio.outputLatency + 0.2)
            }
        case "output_audio_buffer.cleared":
            history.interrupt(id)
            if id == activeResponse { _ = stopPlayback(); phase = .listening }
        case "response.done":
            guard let response = event["response"] as? [String: Any] else { return }
            let responseID = response["id"] as? String ?? ""
            guard response["status"] as? String == "completed", !ignored.contains(responseID) else {
                history.interrupt(responseID)
                if responseID == activeResponse {
                    _ = stopPlayback(); phase = .listening
                    status = response["status"] as? String == "failed" ? "这次回复未能完成，可以再试一次" : "我在听"
                }
                return
            }
            let allCalls = (response["output"] as? [[String: Any]] ?? []).filter { $0["type"] as? String == "function_call" }
            let calls = allCalls.filter { $0["name"] as? String == "set_behavior" }
            guard calls.count <= 8 else { suspend("角色动作请求过多，请重新接通"); return }
            for item in calls {
                guard let callID = item["call_id"] as? String else { continue }
                let applied = !userSpeaking && applyBehavior(item)
                try rtc.send(["type": "conversation.item.create", "item": ["type": "function_call_output", "call_id": callID,
                    "output": applied ? "{\"applied\":true}" : "{\"applied\":false}"]])
            }
            if !calls.isEmpty && calls.count == allCalls.count && !userSpeaking { try rtc.send(["type": "response.create"]) }
        case "error": suspend("GPT 通话发生错误，请检查服务端配置后重新接通")
        default: break
        }
    }

    func applyBehavior(_ item: [String: Any]) -> Bool {
        guard !userSpeaking, item["name"] as? String == "set_behavior", let text = item["arguments"] as? String, text.utf8.count <= 1024,
              let data = text.data(using: .utf8), let args = try? JSONSerialization.jsonObject(with: data) as? [String: String],
              Set(args.keys) == ["emotion", "gesture"], let emotion = args["emotion"], let gesture = args["gesture"] else { return false }
        return avatar.behave(emotion: emotion, gesture: gesture)
    }

}

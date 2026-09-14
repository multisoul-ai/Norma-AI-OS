import Foundation
import NormaCore

extension VoiceCall {
    func startReportObservation(client: RealtimeBindingClient, target: Target) {
        reportTask?.cancel()
        guard let route = target.project.routeID else { reportStatus = "项目未打开，暂时不能接收成果汇报"; return }
        let current = generation
        reportTask = Task {
            do {
                try await reports.load()
                let scope = ProjectReviewRequest(workspaceID: target.project.id, routeID: route)
                for try await update in client.deliveryUpdates(scope: scope) {
                    guard current == generation, wantsCall, !Task.isCancelled else { return }
                    switch update {
                    case .current(let page):
                        reportStatus = "正在核对成果汇报"
                        try await refreshReports(page, client: client, scope: scope, target: target)
                        guard current == generation, wantsCall, !Task.isCancelled else { return }
                        reportStatus = provider == .doubao && !doubaoReports ? "当前豆包网关暂不支持任务语音汇报；结果已保留在汇报记录。" : nil
                    case .recovering: reportStatus = "成果连接中断，正在核对；暂不播报旧状态"
                    case .legacy: reportStatus = "电脑版本暂不支持实时成果汇报"
                    }
                }
            } catch {
                if current == generation, !Task.isCancelled { reportStatus = "成果尚未同步，请在成果记录中核对。" }
            }
        }
    }

    private func refreshReports(_ first: TaskDeliveryPage, client: RealtimeBindingClient,
                                scope: ProjectReviewRequest, target: Target) async throws {
        let previous = reports.queue.entries.filter { $0.machineID == target.machine.id && $0.workspaceID == target.project.id }
        let newest = previous.map(\.sequence).max() ?? 0
        var needed = Set(previous.filter { $0.playback != .played }.map(\.id))
        var items: [TaskDeliverySummary] = [], page = first
        while true {
            for item in page.items {
                needed.remove(target.machine.id + "/" + item.workspaceID + "/" + item.fingerprint)
            }
            items += page.items
            guard let cursor = page.nextCursor, !needed.isEmpty || (page.items.last?.sequence ?? 0) > newest else { break }
            // 128 is the durable queue's explicit ceiling; refuse overflow instead of dropping an event gap.
            guard items.count < 128 else { throw MobileError.unavailable }
            page = try await client.deliveryPage(scope: scope, before: cursor)
            try Task.checkCancellation()
        }
        guard needed.isEmpty else { throw MobileError.unavailable } // Do not announce cached results as freshly verified.
        for delivery in items.reversed() {
            try reports.observe(machineID: target.machine.id,
                name: target.machine.name + " · " + target.project.displayName, delivery: delivery)
        }
    }

    func setReportsPaused(_ paused: Bool) {
        reportsPaused = paused
    }

    func beginReportIfReady() {
        guard (provider == .openai && realtime != nil) || (provider == .doubao && doubaoReports && doubaoSender != nil),
              ProcessInfo.processInfo.systemUptime >= reportRetryAfter,
              state == .connected, (phase == .listening || (muted && phase == .idle)), !userSpeaking,
              activeResponse.isEmpty, reportBatch == nil, reportStatus == nil,
              let target,
              let batch = reports.begin(machineID: target.machine.id, workspaceID: target.project.id,
                                        speaking: userSpeaking, paused: reportsPaused) else { return }
        reportBatch = batch
        let current = generation
        reportSendTask = Task {
            do {
                try await reports.flush()
                guard current == generation, reportBatch?.id == batch.id, !userSpeaking,
                      !reportsPaused, !Task.isCancelled, activeResponse.isEmpty else { return }
                // Out-of-band summary cannot trigger tools or alter the user's task conversation.
                if provider == .doubao {
                    doubaoSender?.send(["type":"gateway.report", "report_id":batch.id, "text":batch.text])
                } else { try realtime?.send(["type":"response.create", "event_id":batch.id, "response":[
                    "conversation":"none", "metadata":["norma_report":batch.id], "input":[],
                    "output_modalities":["audio"], "tools":[], "tool_choice":"none",
                    "max_output_tokens":128, "instructions":"请只照读以下任务结果，保持简短，不添加推断：" + batch.text
                ]]) }
                // 15s is response-start timeout. Actual speech gets its separate 10s ceiling below.
                reportDeadline = Task {
                    try? await Task.sleep(for: .seconds(15))
                    if !Task.isCancelled, reportBatch?.id == batch.id { stopUnconfirmedReport() }
                }
            } catch {
                if reportBatch?.id == batch.id { reports.unconfirmed(batch); interruptReport() }
                reportStatus = "本次汇报未能开始，结果仍在记录中。"
            }
        }
    }

    func boundReportDuration() {
        guard let batch = reportBatch else { return }
        reportDeadline?.cancel()
        reportDeadline = Task {
            // Ordinary reports must not occupy more than about ten seconds of the call.
            try? await Task.sleep(for: .seconds(10))
            if !Task.isCancelled, reportBatch?.id == batch.id { stopUnconfirmedReport() }
        }
    }
    private func stopUnconfirmedReport() {
        if let batch = reportBatch { reports.unconfirmed(batch) }
        interrupt()
        reportStatus = "汇报播放尚未确认，结果仍在记录中。"
    }
    func interruptReport() {
        reportSendTask?.cancel(); reportSendTask = nil
        reportDeadline?.cancel(); reportDeadline = nil
        if let batch = reportBatch { reports.interrupt(batch) }
        reportBatch = nil; reportResponse = ""
    }
    func finishReportPlayback(response: String) {
        guard !response.isEmpty, response == reportResponse, let batch = reportBatch else { return }
        reports.played(batch)
        interruptReport() // The queue's completed playback state cannot be changed back to waiting.
    }
}

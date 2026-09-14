import XCTest
import NormaCore
@testable import Norma

@MainActor final class RealtimeReportCallTests: XCTestCase {
    /// 豆包汇报必须保留等待中的批次，并在真实PCM播放耗尽后确认；暂停不能被迟到音频撤销。
    /// 数据构造：4800采样=24000Hz×0.2s，PCM16=9600bytes；两份成果分别来自真实队列状态。
    /// 执行过程：准备批次A → 带report_id的started → 实际播放PCM并done → tick确认 →
    ///   准备批次B → 设置暂停 → 用户讲话中注入迟到started → 用户停讲后注入旧PCM。
    /// 预期结果：A已播但未验收；B待播且无活动回复；网络done不能单独表示播放完成。
    func testDoubaoReportWaitsForActualPCMPlayback() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let call = VoiceCall(discussionURL: directory.appendingPathComponent("drafts.json"),
            operationURL: directory.appendingPathComponent("operations.json"), reportURL: directory.appendingPathComponent("reports.json"))
        defer { call.hangup() }
        call.provider = .doubao; call.state = .connected
        try await call.reports.load()
        let machine = "mac-" + String(repeating:"a",count:64)
        func prepare(_ key: String) async throws -> RealtimeReportQueue.Batch {
            let delivery = try JSONDecoder().decode(TaskDeliverySummary.self, from: JSONSerialization.data(withJSONObject:[
                "sequence":1,"workspace_id":"project-a","fingerprint":String(repeating:key,count:64),
                "saved_at_unix":100,"status":"available","file_count":1,
                "outcome":["key":["node_id":7,"profile":"Codex","run_id":"run-a","session_id":"session-a","turn_id":"turn-"+key],"state":"completed"]]))
            // Six seconds elapsed exceeds the queue's five-second coalescing interval.
            try call.reports.observe(machineID:machine,name:"测试项目",delivery:delivery,now:.now.addingTimeInterval(-6))
            let batch = try XCTUnwrap(call.reports.begin(machineID:machine,workspaceID:"project-a",speaking:false,paused:false))
            call.reportBatch=batch; try await call.reports.flush(); return batch
        }
        let first=try await prepare("a")
        try await call.handleDoubao(["type":"response.output_audio.started","response_id":"pcm-report-a","report_id":first.id],current:call.generation)
        XCTAssertEqual(call.reportResponse,"pcm-report-a","汇报开始不能通过stopPlayback丢掉待播批次")
        XCTAssertEqual(call.reports.queue.entries[0].playback,.speaking,"收到started还不能认为已播")
        try await call.audio.start(speaker:false,owner:call.generation)
        let samples=(0..<4800).map { Int16(sin(Double($0)*2*Double.pi*440/24000)*1000) }
        let pcm=samples.withUnsafeBytes { Data($0) }
        try await call.handleDoubao(["type":"response.output_audio.delta","response_id":"pcm-report-a","delta":pcm.base64EncodedString()],current:call.generation)
        try await wait { await call.audio.snapshot().playedMS > 0 }
        try await call.handleDoubao(["type":"response.output_audio.done","response_id":"pcm-report-a"],current:call.generation)
        try await wait { await call.tick(); return call.reports.queue.entries[0].playback == .played }
        XCTAssertFalse(call.reports.queue.entries[0].accepted,"播放确认不能替用户接受成果")
        let second=try await prepare("b")
        try await call.handleDoubao(["type":"response.output_audio.started","response_id":"pcm-report-b","report_id":second.id],current:call.generation)
        call.reportsPaused=true
        call.userSpeaking=true
        try await call.handleDoubao(["type":"response.output_audio.started","response_id":"pcm-late","report_id":second.id],current:call.generation)
        call.userSpeaking=false
        try await call.handleDoubao(["type":"response.output_audio.delta","response_id":"pcm-late","delta":pcm.base64EncodedString()],current:call.generation)
        XCTAssertEqual(call.reports.queue.entries[1].playback,.waiting,"暂停后批次仍须待播")
        XCTAssertTrue(call.activeResponse.isEmpty,"迟到汇报不能复活已暂停播放")
        await call.audio.stop()
        try await call.reports.flush()
    }

    /// 汇报经过真实本地WebRTC音频/控制通道；实际播放完成才消费，插话与迟到响应不能复活汇报。
    /// 数据构造：本机UDP peer发送48kHz/10ms/480采样、振幅800/32768的测试音；两份隔离成果。
    ///   最多100×50ms=5秒等待ICE/硬件就绪；每项收到时间设为now-6秒，超过5秒合并期。
    /// 执行过程：真实SDP/SCTP → 开始汇报A并实际渲染 → 停音且缓冲停止 → tick确认已播 →
    ///   汇报B播放中设置页绑定暂停 → 发送旧stopped和迟到created → 核对真实取消事件及播放状态。
    /// 预期结果：A已播但未验收，B仍待播；迟到响应被取消且嘴型归零；不连接云端、不派工。
    func testReportPlaybackAndLateCancellationOverNativePeer() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let call = VoiceCall(discussionURL: directory.appendingPathComponent("drafts.json"),
            operationURL: directory.appendingPathComponent("operations.json"),
            reportURL: directory.appendingPathComponent("reports.json"))
        let rtc = OpenAIRealtime(), remote = ReportLocalPeer()
        defer { call.hangup(); rtc.close(); remote.close() }
        call.provider = .openai; call.realtime = rtc; call.state = .connected
        var connected = false, failures: [String] = [], cancelled: [String] = []
        rtc.onEvent = { event in
            if event["type"] as? String == "session.created" { connected = true; return }
            do { try call.handleOpenAI(event, current: call.generation) }
            catch { failures.append(error.localizedDescription) }
        }
        remote.onEvent = { event in
            if event["type"] as? String == "response.cancel", let id = event["response_id"] as? String { cancelled.append(id) }
        }
        try rtc.prepare(speaker: false)
        _ = try await rtc.makeOffer()
        try await wait { rtc.peer?.iceGatheringState == .complete }
        let offer = try XCTUnwrap(rtc.peer?.localDescription, "真实客户端必须提供SDP")
        try await rtc.acceptAnswer(remote.answer(offer))
        try await wait { connected }
        try await call.reports.load()
        let machine = "mac-" + String(repeating: "a", count: 64)
        func prepare(_ key: String) async throws -> RealtimeReportQueue.Batch {
            let delivery = try JSONDecoder().decode(TaskDeliverySummary.self, from: JSONSerialization.data(withJSONObject: [
                "sequence":1,"workspace_id":"project-a","fingerprint":String(repeating:key,count:64),
                "saved_at_unix":100,"status":"available","file_count":1,
                "outcome":["key":["node_id":7,"profile":"Codex","run_id":"run-a","session_id":"session-a","turn_id":"turn-"+key],"state":"completed"]
            ]))
            try call.reports.observe(machineID: machine, name: "测试项目", delivery: delivery, now: .now.addingTimeInterval(-6))
            let batch = try XCTUnwrap(call.reports.begin(machineID: machine, workspaceID: "project-a", speaking: false, paused: false))
            call.reportBatch = batch
            try await call.reports.flush()
            return batch
        }
        let first = try await prepare("a")
        try remote.send(["type":"response.created", "response":["id":"report-a","metadata":["norma_report":first.id]]])
        try remote.send(["type":"output_audio_buffer.started","response_id":"report-a"])
        try await wait { rtc.audio.snapshot().level > 0.01 && rtc.audio.snapshot().rendered > 0 }
        XCTAssertGreaterThan(rtc.audio.snapshot().captured, 0, "真实麦克风回调必须运行")
        XCTAssertEqual(call.reports.queue.entries[0].playback, .speaking, "有音频正在播放时不能提前确认")
        remote.setTone(false)
        try remote.send(["type":"output_audio_buffer.stopped","response_id":"report-a"])
        try await wait {
            await call.tick()
            return call.reports.queue.entries[0].playback == .played
        }
        XCTAssertFalse(call.reports.queue.entries[0].accepted, "播放完成不能替用户验收")
        let second = try await prepare("b")
        remote.setTone(true)
        try remote.send(["type":"response.created","response":["id":"report-b","metadata":["norma_report":second.id]]])
        try remote.send(["type":"output_audio_buffer.started","response_id":"report-b"])
        try await wait { call.reportResponse == "report-b" && rtc.audio.snapshot().level > 0.01 }
        // CallSettingsView writes this property directly through its native Toggle binding.
        call.reportsPaused = true
        try remote.send(["type":"output_audio_buffer.stopped","response_id":"report-b"])
        try remote.send(["type":"response.created","response":["id":"late-b","metadata":["norma_report":second.id]]])
        try await wait { cancelled.contains("late-b") }
        await call.tick()
        XCTAssertTrue(cancelled.contains("report-b"), "设置页暂停汇报必须通过SCTP取消原回复")
        XCTAssertTrue(call.reportsPaused, "实际暂停偏好必须保留，不能自动恢复汇报")
        XCTAssertEqual(call.reports.queue.entries[1].playback, .waiting, "未播完不能被迟到停止事件消费")
        XCTAssertTrue(call.activeResponse.isEmpty, "旧汇报不能成为新的活动回复")
        XCTAssertEqual(rtc.audio.snapshot().level, 0, "真实播放打断后嘴型必须归零")
        XCTAssertTrue(failures.isEmpty, "所有控制事件都应成功处理：\(failures)")
        try await call.reports.flush()
    }
    private func wait(_ predicate: @MainActor () async throws -> Bool) async throws {
        for _ in 0..<100 {
            if try await predicate() { return }
            try await Task.sleep(for: .milliseconds(50))
        }
        XCTFail("本机WebRTC流程在5秒内未达到预期状态")
        throw OpenAIRealtime.RTCError.signaling
    }
}

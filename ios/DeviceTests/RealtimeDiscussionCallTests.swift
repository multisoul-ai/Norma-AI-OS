import XCTest
import NormaCore
@testable import Norma

@MainActor final class RealtimeDiscussionCallTests: XCTestCase {
    /// 草稿写入失败后暂停通话，保留未保存原话，不能持续采集并无限累积内存。
    /// 数据构造：隔离损坏JSON和一句原话；回环discard端口9不会连接任何外部语音服务。
    /// 执行过程：主动connect设置通话意图 → completed保存失败 → 生产tick检查 → 核对暂停及原文件。
    /// 预期结果：无继续通话意图、无麦克风捕获任务；原话仍在内存且坏文件字节不变。
    func testUnsaveableDiscussionPausesCaptureWithoutLosingOriginal() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("discussions.json"), original = Data("damaged".utf8)
        try original.write(to: url)
        let call = VoiceCall(discussionURL: url, operationURL: directory.appendingPathComponent("operations.json"),
            reportURL: directory.appendingPathComponent("reports.json"))
        defer { call.hangup() }
        call.gateway = "http://127.0.0.1:9"; call.accessToken = "local-discard-test-passphrase"
        call.connect(machine: nil, project: nil)
        XCTAssertTrue(call.isCalling, "必须先经过真实通话意图入口，不能手动伪造状态")
        call.recordDiscussion(["transcript":"保留这句原话。"])
        do { try await call.discussions.flush(); XCTFail("损坏的文件不得保存成功") } catch {}
        await call.tick()
        XCTAssertEqual(call.state, .paused, "写入失败必须停止继续采集")
        XCTAssertFalse(call.isCalling, "不能在草稿不可保存时无限继续通话")
        XCTAssertNil(call.captureTask, "暂停必须取消捕获任务")
        XCTAssertEqual(call.discussions.entries.first?.text, "保留这句原话。", "失败原话必须仍可查看")
        XCTAssertEqual(try Data(contentsOf: url), original, "不能用新原话覆盖损坏文件")
    }

    /// 正式豆包事件处理保存完整原话，挂断重建通话对象仍能读取，且不创建媒体或Mac任务。
    /// 数据构造：重复中文原话200次，长度超过字幕300字符上限；独立临时JSON，不使用真实草稿。
    /// 执行过程：向正式handler交付开始/完成事件 → 挂断 → 新建VoiceCall并读取同一store。
    /// 预期结果：原话完整保留、无第二份；没有socket/sideband，恢复不自动开聊。
    func testCompletedSpeechPersistsOutsideCallLifecycle() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("discussions.json")
        let call = VoiceCall(discussionURL: url)
        let text = String(repeating: "这个思路不错，先记下来。", count: 200)
        try await call.handleDoubao(["type": "conversation.item.input_audio_transcription.started", "item_id": "utterance-a"], current: call.generation)
        try await call.handleDoubao(["type": "conversation.item.input_audio_transcription.completed", "item_id": "utterance-a", "transcript": text], current: call.generation)
        try await call.handleDoubao(["type": "conversation.item.input_audio_transcription.completed", "item_id": "utterance-a", "transcript": text], current: call.generation)
        call.hangup()
        try await call.discussions.flush()
        let restored = VoiceCall(discussionURL: url)
        await restored.discussions.load()
        XCTAssertEqual(restored.discussions.entries.first?.text, text, "草稿保存原文，不能保存裁剪字幕")
        XCTAssertEqual(restored.discussions.entries.count, 1, "重复完成事件只保留一次")
        XCTAssertFalse(restored.isCalling, "草稿恢复不能主动开启付费通话")
        XCTAssertNil(restored.socket, "读取本地讨论不能创建语音连接")
        XCTAssertNil(restored.macBindingTask, "重启不能把讨论转成任务")
    }

    /// 用户说稍后一起汇报只暂停播报，明确恢复后继续；两句控制原话仍保存。
    /// 数据构造：两句带中文句号的完整转写、隔离JSON；没有媒体连接或执行任务。
    /// 执行过程：完成转写“等会儿一起说” → 暂停 → 完成转写“继续汇报” → 恢复并落盘。
    /// 预期结果：暂停及恢复状态正确；保留两句原文，不产生Mac执行连接。
    func testSpokenReportControlsKeepOriginalDiscussion() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let call = VoiceCall(discussionURL: directory.appendingPathComponent("discussions.json"),
            operationURL: directory.appendingPathComponent("operations.json"), reportURL: directory.appendingPathComponent("reports.json"))
        call.recordDiscussion(["transcript":"等会儿一起说。"])
        XCTAssertTrue(call.reportsPaused, "完整的稍后汇报请求必须暂停播报")
        call.recordDiscussion(["transcript":"继续汇报。"])
        XCTAssertFalse(call.reportsPaused, "明确恢复后不能仍保持暂停")
        try await call.discussions.flush()
        XCTAssertEqual(call.discussions.entries.map(\.text), ["等会儿一起说。", "继续汇报。"], "控制意图也应保留原话")
        XCTAssertNil(call.macBindingTask, "汇报控制不得触发Mac任务执行")
    }
}

import XCTest
@testable import NormaCore

final class RealtimeReportTests: XCTestCase {
    /// 崩溃时正在播报的条目重启后保持“播放待核对”，不能自动重复；明确恢复后才可再次汇报。
    /// 数据构造：一份真实结构的完成成果、临时JSON；收到时间now-6秒已超过5秒合并期。
    /// 执行过程：排队并开始 → 落盘但不确认播放 → 新book读取 → 自动开始被挡 → 明确恢复并播完。
    /// 预期结果：重启不自动复播；恢复保留原成果身份；已播落盘后重启仍不再播，验收状态不变。
    @MainActor func testRestartDoesNotReplayUnconfirmedPlayback() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("reports.json"), machine = "mac-" + String(repeating: "a", count: 64)
        let book = RealtimeReportBook(store: RealtimeReportStore(url: url))
        try await book.load()
        try book.observe(machineID: machine, name: "测试项目", delivery: delivery("a", at: 100), now: .now.addingTimeInterval(-6))
        XCTAssertNotNil(book.begin(machineID: machine, workspaceID: "project-a", speaking: false, paused: false), "首次可以开始")
        try await book.flush()
        let recovered = RealtimeReportBook(store: RealtimeReportStore(url: url))
        try await recovered.load()
        XCTAssertEqual(recovered.queue.entries[0].playback, .unconfirmed, "进程退出时无法证明实际播完")
        XCTAssertNil(recovered.begin(machineID: machine, workspaceID: "project-a", speaking: false, paused: false), "未知播放结果不能自动重复")
        recovered.resume()
        let batch = try XCTUnwrap(recovered.begin(machineID: machine, workspaceID: "project-a", speaking: false, paused: false))
        recovered.played(batch); try await recovered.flush()
        let final = try await RealtimeReportStore(url: url).load()
        XCTAssertEqual(final.entries[0].playback, .played, "实际播放完成必须持久保存")
        XCTAssertFalse(final.entries[0].accepted, "持久播放结果不能替用户通过验收")
    }
    /// 完成事件等5秒合并，用户讲话或暂停时不开始；只有实际播放完成才消费。
    /// 数据构造：t=100秒收到A，t=102秒收到B；合并门限100+5=105，首批2个完成且均未验收。
    /// 执行过程：t104尝试 → t105讲话/暂停 → 安静后开始 → 插话 → 再开始并实际播完 → 重复事件。
    /// 预期结果：提前/讲话/暂停都无播报；插话不丢队列；已播不重复，任务完成不代表验收通过。
    func testCoalescingAndPlaybackAreSeparateFromTaskAcceptance() throws {
        var queue = RealtimeReportQueue()
        let machine = "mac-" + String(repeating: "a", count: 64)
        try queue.observe(machineID: machine, name: "Mac A · 项目A", delivery: delivery("a", at: 100), now: Date(timeIntervalSince1970: 100))
        try queue.observe(machineID: machine, name: "Mac A · 项目A", delivery: delivery("b", at: 102), now: Date(timeIntervalSince1970: 102))
        func time(_ value: Double) -> Date { Date(timeIntervalSince1970: value) }
        XCTAssertNil(queue.begin(machineID: machine, workspaceID: "project-a", now: time(104), speaking: false, paused: false), "5秒之前应等待合并")
        XCTAssertNil(queue.begin(machineID: machine, workspaceID: "project-a", now: time(105), speaking: true, paused: false), "用户说话时不能抢话")
        XCTAssertNil(queue.begin(machineID: machine, workspaceID: "project-a", now: time(105), speaking: false, paused: true), "暂停只暂停汇报")
        let first = try XCTUnwrap(queue.begin(machineID: machine, workspaceID: "project-a", now: time(105), speaking: false, paused: false))
        XCTAssertEqual(first.ids.count, 2, "两项完成应合并成一次播报")
        XCTAssertTrue(first.text.contains("2项执行完成"), "文本必须来自真实完成计数")
        XCTAssertFalse(queue.entries[0].accepted, "语音排队不等于用户验收")
        queue.interrupt(first)
        let second = try XCTUnwrap(queue.begin(machineID: machine, workspaceID: "project-a", now: time(106), speaking: false, paused: false))
        queue.played(second)
        try queue.observe(machineID: machine, name: "Mac A · 项目A", delivery: delivery("a", at: 100), now: time(107))
        XCTAssertNil(queue.begin(machineID: machine, workspaceID: "project-a", now: time(120), speaking: false, paused: false), "已播放的重复事件不得再次播报")
        XCTAssertEqual(queue.entries[1].playback, .played, "实际播放完成标记必须独立记录")
        XCTAssertFalse(queue.entries[1].accepted, "实际播放不能替用户通过验收")
    }
    private func delivery(_ fingerprint: String, at timestamp: UInt64) throws -> TaskDeliverySummary {
        try JSONDecoder().decode(TaskDeliverySummary.self, from: JSONSerialization.data(withJSONObject: [
            "sequence":timestamp,"workspace_id":"project-a","fingerprint":String(repeating: fingerprint, count: 64),
            "saved_at_unix":timestamp,"status":"available","file_count":1,
            "outcome":["key":["node_id":7,"profile":"Codex","run_id":"run-a","session_id":"session-a","turn_id":"turn-" + fingerprint],"state":"completed"]
        ]))
    }
}

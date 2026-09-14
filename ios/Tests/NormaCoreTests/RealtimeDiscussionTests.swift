import XCTest
@testable import NormaCore

@MainActor final class RealtimeDiscussionTests: XCTestCase {
    /// 讨论按说话时的项目保存，重复转写和重启只恢复同一份草稿。
    /// 数据构造：两台由64位十六进制身份区分的Mac；两个项目各一句原话，无卡片或派工命令。
    /// 执行过程：保存A想法 → 保存B想法 → 重复A → 新建store和book读取 → 删除A。
    /// 预期结果：A/B各保留一次且归属不变；恢复不产生发送动作；删除A不删除B。
    func testDiscussionSurvivesRestartWithoutMergingProjects() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("discussion.json")
        let a = RealtimeDiscussion(id: "utterance-a", machineID: "mac-" + String(repeating: "a", count: 64),
            workspaceID: "project-a", targetName: "Mac A · 项目A", text: "这个思路不错，先记下来。", recordedAt: .now)
        let b = RealtimeDiscussion(id: "utterance-b", machineID: "mac-" + String(repeating: "b", count: 64),
            workspaceID: "project-b", targetName: "Mac B · 项目B", text: "另一个项目可以试试蓝色。", recordedAt: .now)
        let book = RealtimeDiscussionBook(store: RealtimeDiscussionStore(url: url))
        book.record(a); book.record(b); book.record(a)
        try await book.flush()
        let restored = RealtimeDiscussionBook(store: RealtimeDiscussionStore(url: url))
        await restored.load()
        XCTAssertEqual(restored.entries.count, 2, "重连重复事件不能生成第二份草稿")
        XCTAssertEqual(restored.entries.first { $0.id == a.id }?.workspaceID, "project-a", "浏览项目B不能迁移A原话")
        XCTAssertEqual(restored.entries.first { $0.id == b.id }?.text, b.text, "原文不能被字幕300字符上限裁剪")
        XCTAssertFalse(restored.entries.contains { $0.id == "operation-a" }, "草稿恢复不生成任何派工操作")
        await restored.remove(id: a.id)
        try await restored.flush()
        let remaining = try await RealtimeDiscussionStore(url: url).load()
        XCTAssertFalse(remaining.contains { $0.id == a.id }, "明确删除的A不能复活")
        XCTAssertEqual(remaining.first?.id, b.id, "删除A必须保留B")
    }

    /// 损坏的磁盘草稿不能被新转写覆盖，失败原话留在内存等待重试。
    /// 数据构造：1个无效JSON文件和1句未绑定项目的原话；不造虚假node/workspace身份。
    /// 执行过程：写坏文件 → record新原话 → flush失败 → 核对磁盘及内存。
    /// 预期结果：文件字节不变，原话仍可查看；保存失败可见，不能假报已保存。
    func testDamagedFileRetainsUnsavedSpeech() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("discussion.json"), damaged = Data("damaged".utf8)
        try damaged.write(to: url)
        let book = RealtimeDiscussionBook(store: RealtimeDiscussionStore(url: url))
        book.record(RealtimeDiscussion(id: "offline", machineID: nil, workspaceID: nil,
            targetName: "未绑定项目", text: "离线也要保留这个想法。", recordedAt: .now))
        do { try await book.flush(); XCTFail("损坏文件不能被标记保存成功") } catch {}
        XCTAssertEqual(try Data(contentsOf: url), damaged, "原始坏文件必须保留供恢复")
        XCTAssertEqual(book.entries.first?.text, "离线也要保留这个想法。", "未落盘原话不能丢失")
        XCTAssertNotNil(book.failure, "保存失败必须对用户可见")
        XCTAssertFalse(book.loaded, "损坏记录不能被当成空历史")
    }
}

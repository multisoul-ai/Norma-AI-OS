import Foundation
import XCTest
@testable import NormaCore

final class CloudChatStoreTests: XCTestCase {
    /// Norma 私聊先保存用户原文，完整云回复跨重启保留；中断回合不伪装成已完成。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   2 个回合 = 1 个成功回合（用户 + Norma 共 2 条消息）+ 1 个失败回合（只有用户 1 条）。
    ///   两次保存之间相隔 10 秒，使列表顺序可以按明确时间判断。
    ///
    /// 执行过程：
    ///   1. 先落用户原文，核对 pending；写入完整 Norma 回复。
    ///   2. 第二轮仅写用户原文并标记服务中断；重建 Store。
    ///   3. 搜索历史并检查每条身份、状态与内容。
    ///
    /// 预期结果：成功轮的两条消息长期保留；失败轮只保留原文和失败状态，没有伪造的 Norma 回复。
    func testPrivateChatPersistsCompletedAndInterruptedTurns() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("norma-chat.sqlite")
        let store = CloudChatStore(databaseURL: url)
        let first = "00000000-0000-0000-0000-000000000001"
        let second = "00000000-0000-0000-0000-000000000002"
        let time = Date(timeIntervalSince1970: 1_000)
        let user = try await store.appendUser(turnID: first, text: "首屏怎么改？", at: time)
        XCTAssertEqual(user.state, .pending, "发送前必须先持久记录待回复用户消息")
        _ = try await store.complete(turnID: first, text: "先突出下一步。", at: time.addingTimeInterval(10))
        _ = try await store.appendUser(turnID: second, text: "继续聊交互", at: time.addingTimeInterval(20))
        try await store.fail(turnID: second, code: "provider_unavailable")

        let reopened = CloudChatStore(databaseURL: url)
        let history = try await reopened.all()
        XCTAssertEqual(history.count, 3, "重启后必须保留成功轮 2 条和失败轮 1 条")
        XCTAssertEqual(history[0].id, user.id, "第一条用户消息的稳定 ID 不得变化")
        XCTAssertEqual(history[0].state, .complete, "有完整云回复的用户消息才可标为完成")
        XCTAssertEqual(history[1].role, .norma, "第二条必须明确来自云端 Norma")
        XCTAssertEqual(history[1].text, "先突出下一步。", "云端可见回复必须原样保留")
        XCTAssertEqual(history[2].turnID, second, "失败记录必须保持自己的回合身份")
        XCTAssertEqual(history[2].state, .failed, "中断后不得继续显示发送中或完成")
        XCTAssertEqual(history[2].errorCode, "provider_unavailable", "失败原因应保留供用户重试判断")
        XCTAssertFalse(history.contains { $0.turnID == second && $0.role == .norma },
                       "失败轮不得伪造云端回复")
        let matches = try await reopened.search("首屏")
        XCTAssertEqual(matches.map(\.id), [user.id], "本机搜索只应返回包含首屏的私聊消息")
        let unrelated = try await reopened.search("不存在的词")
        XCTAssertTrue(unrelated.isEmpty, "本机搜索不能返回不相关的私聊内容")
    }
}

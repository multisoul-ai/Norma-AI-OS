import XCTest
@testable import NormaCore

final class ConversationTests: XCTestCase {
    /// 对话来源必须准确；更新同一消息不能重复显示，较早消息插到前面，断层明确重置。
    ///
    /// 数据构造：run-a/session-a、node=7；两页 a→b / b→c，重复 b 从草稿变完整；跨20回合断层由无交集页代表。
    /// 执行过程：校验响应 → 替换 run → 合并首次/更新/较早页 → 插入断层页。
    /// 预期结果：错误运行拒绝；稳定 ID 去重且正文更新；较早消息保留；断层不拼成假连续历史。
    func testConversationIdentityAndTimelineRecovery() throws {
        let request = ConversationRequest(workspaceID: "project-a", routeID: "route-a", nodeID: 7, runID: "run-a", sessionID: "session-a", cursor: nil)
        let raw = #"{"request":{"workspace_id":"project-a","route_id":"route-a","node_id":7,"run_id":"run-a","session_id":"session-a","cursor":null},"page":{"items":[{"id":"a","role":"assistant","text":"完整正文","details":null}],"cursor":"older","has_more":true,"pending_tail":false,"direction":"older"}}"#
        let response = try JSONDecoder().decode(ConversationResponse.self, from: Data(raw.utf8))
        try response.validate(for: request)
        let wrong = try JSONDecoder().decode(ConversationResponse.self, from: Data(raw.replacingOccurrences(of: "run-a", with: "run-b").utf8))
        XCTAssertThrowsError(try wrong.validate(for: request), "另一运行的对话不得显示在此卡片")
        func page(_ ids: [String], text: String = "旧", cursor: String? = "older") -> ConversationPage {
            ConversationPage(items: ids.map { ConversationItem(id: $0, role: "assistant", text: text, details: nil) }, cursor: cursor, hasMore: cursor != nil, pendingTail: false, direction: "older")
        }
        var timeline = ConversationTimeline()
        XCTAssertFalse(timeline.merge(page(["a", "b"]), continuing: false), "首屏没有断层")
        XCTAssertFalse(timeline.merge(page(["b", "c"], text: "完整"), continuing: false), "有交集的增量不能重置")
        XCTAssertEqual(timeline.items.map(\.id), ["a", "b", "c"], "b 不重复，c 追加")
        XCTAssertEqual(timeline.items[1].text, "完整", "同 ID 的流式正文应更新")
        XCTAssertFalse(timeline.merge(page(["older", "a"], cursor: nil), continuing: true), "加载前页保持连续")
        XCTAssertEqual(timeline.items.first?.id, "older", "较早消息应前插")
        XCTAssertTrue(timeline.merge(page(["far-new"]), continuing: false), "无交集且仍有前页表示断层")
        XCTAssertEqual(timeline.items.map(\.id), ["far-new"], "断层后先重建可验证页，不伪造无缺口历史")
        XCTAssertFalse(timeline.items.contains { $0.id == "a" }, "断层前旧内容不能继续冒充连续列表")
    }
}

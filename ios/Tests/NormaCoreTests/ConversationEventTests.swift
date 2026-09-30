import XCTest
@testable import NormaCore

final class ConversationEventTests: XCTestCase {
    /// 对话增量必须有本连接基线；断层、错run清空缓存，心跳不再次发布正文。
    ///
    /// 数据构造：request绑定run-a；stream-a sequence0正文、sequence1增长、sequence3缺口。
    /// 执行过程：首屏 → 心跳 → 更新 → 心跳缺口 → 无基线心跳 → 新流基线 → 正文缺口 → 另一run事件。
    /// 预期结果：心跳不产生新页面但仍严格校验身份/序号；正文完整增长，失败清空；不能靠心跳恢复。
    func testConversationEventSequenceAndScope() throws {
        let request=ConversationRequest(workspaceID: "p", routeID: "r", nodeID: 1, runID: "run-a", sessionID: "session-a", cursor: nil)
        func event(_ sequence: UInt64, text: String?, stream: String = "stream-a", scope: ConversationRequest? = nil) -> ConversationEvent {
            ConversationEvent(streamID: stream, sequence: sequence, request: scope ?? request,
                page: text.map { ConversationPage(items: [ConversationItem(id: "one", role: "assistant", text: $0, details: nil)], cursor: nil, hasMore: false, pendingTail: false, direction: "older") })
        }
        var state=ConversationEventSequence(request: request)
        XCTAssertEqual(try state.apply(event(0,text:"中"))?.items[0].text,"中","基线正文完整")
        XCTAssertNil(try state.apply(event(0,text:nil)),"心跳不能再次发布完整正文并触发页面合并")
        XCTAssertEqual(state.page?.items[0].text,"中","心跳仍保留已验证内容，不清空缓存")
        XCTAssertEqual(try state.apply(event(1,text:"中文"))?.items[0].text,"中文","心跳后的真实增长仍必须立即发布")
        XCTAssertThrowsError(try state.apply(event(2,text:nil)),"不带正文的错误心跳序号也必须触发缺口")
        XCTAssertNil(state.page,"无效心跳必须清理旧页，不能因无正文而跳过校验")
        XCTAssertThrowsError(try state.apply(event(0,text:nil)),"无基线不能接受心跳并伪装为在线内容")
        XCTAssertEqual(try state.apply(event(0,text:"重建"))?.items[0].text,"重建","缺口后需要完整新基线")
        XCTAssertThrowsError(try state.apply(event(3,text:"错误")),"序号缺口必须重建")
        XCTAssertNil(state.page,"缺口后不保留可显示的旧页")
        XCTAssertThrowsError(try state.apply(event(4,text:"仍错误")),"无新基线不能续用增量")
        XCTAssertEqual(try state.apply(event(0,text:"恢复",stream:"stream-b"))?.items[0].text,"恢复","新流从零恢复")
        let other=ConversationRequest(workspaceID: "p", routeID: "r", nodeID: 1, runID: "run-b", sessionID: "session-a", cursor: nil)
        XCTAssertThrowsError(try state.apply(event(1,text:"串线",stream:"stream-b",scope:other)),"旧窗口同卡ID也不能跨run")
        XCTAssertNil(state.page,"错run必须清空")
    }
}

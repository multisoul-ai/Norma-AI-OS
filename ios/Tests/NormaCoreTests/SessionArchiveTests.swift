import XCTest
@testable import NormaCore

final class SessionArchiveTests: XCTestCase {
    /// 归档列表须绑定正确项目与游标，完整会话身份不依赖当前卡片。
    ///
    /// 数据构造：项目A、route-a，已删除的node7，sequence=3；before=4只允许小于4的记录。
    /// 执行过程：解码已归档身份 → 校验项目/顺序 → 构造正文请求 → 更换项目或重放游标。
    /// 预期结果：原身份可读且无虚假下一页；跨项目与重复sequence拒绝，不依赖当前卡片快照。
    func testArchiveScopeAndCursor() throws {
        let scope = ProjectReviewRequest(workspaceID: "project-a", routeID: "route-a")
        let raw = #"{"request":{"workspace_id":"project-a","route_id":"route-a"},"items":[{"sequence":3,"recorded_at_unix":100,"title":"原会话","key":{"node_id":7,"profile":"Claude","run_id":"run-a","session_id":"session-a"}}],"next_cursor":null}"#
        let page = try JSONDecoder().decode(SessionArchivePage.self, from: Data(raw.utf8))
        XCTAssertNoThrow(try page.validate(for: scope, before: 4), "原会话满足范围和分页约束")
        XCTAssertNil(page.nextCursor, "单条记录不应产生虚假分页")
        let request = page.items[0].conversationRequest(scope: scope, cursor: nil)
        XCTAssertEqual(request.nodeID, 7, "删除后仍保留准确原节点")
        XCTAssertEqual(request.runID, "run-a", "不能根据当前卡片替换run")
        XCTAssertThrowsError(try page.validate(for: ProjectReviewRequest(workspaceID: "project-b", routeID: "route-a"), before: 4), "不能跨项目显示")
        XCTAssertThrowsError(try page.validate(for: scope, before: 3), "重复水位不能作为下一页追加")
        let wrong = try JSONDecoder().decode(SessionArchivePage.self, from: Data(raw.replacingOccurrences(of: "Claude", with: "Unknown").utf8))
        XCTAssertThrowsError(try wrong.validate(for: scope, before: 4), "未知Provider不能发起归档读取")
    }
}

import XCTest
@testable import NormaCore

final class ProjectEventTests: XCTestCase {
    /// 增量只应用于已核验的基线，缺口/错项目立即清空；新连接必须用sequence0快照恢复。
    ///
    /// 数据构造：project-a/stream-a初始卡1/2；sequence1删除1、卡2就绪；sequence3为缺口。
    /// 执行过程：初始快照 → delta → 重复delta → 心跳 → 缺口 → 新流snapshot → 错项目。
    /// 预期结果：卡2保留且卡1不存在；重复不执行两遍；缺口及来源错误后没有可操作旧卡。
    func testExactProjectDeltaGapAndSnapshotRecovery() throws {
        let card: [String: Any] = ["node_id": 1, "profile": "Codex", "title": "One", "mobile_control": true, "state": "idle"]
        var second = card; second["node_id"] = 2; second["title"] = "Two"
        let project: [String: Any] = ["workspace_id": "project-a", "display_name": "A", "state": "open", "route_id": "route-a", "card_count": 2, "cards": [card, second]]
        func event(_ extra: [String: Any], stream: String = "stream-a", workspace: String = "project-a") throws -> ProjectEvent {
            var body: [String: Any] = ["stream_id": stream, "workspace_id": workspace]; body.merge(extra) { _, new in new }
            return try JSONDecoder().decode(ProjectEvent.self, from: JSONSerialization.data(withJSONObject: body))
        }
        var timeline = ProjectTimeline(workspaceID: "project-a")
        let initial = try event(["sequence": 0, "kind": "snapshot", "project": project])
        XCTAssertEqual(try timeline.apply(initial).cards?.count, 2, "完整基线包含两张卡")
        second["state"] = "ready"
        let delta = try event(["sequence": 1, "kind": "delta", "display_name": "A", "upserted": [second], "removed": [1], "order": [2]])
        let updated = try timeline.apply(delta)
        XCTAssertEqual(updated.cards?.first?.id, 2, "变化后只保留卡2")
        XCTAssertEqual(updated.cards?.first?.state, "ready", "必须应用真实新状态")
        XCTAssertFalse(updated.cards?.contains { $0.id == 1 } ?? true, "删除卡1不能残留")
        XCTAssertEqual(try timeline.apply(delta).cards?.count, 1, "同一事件重复不能再应用删除")
        XCTAssertEqual(try timeline.apply(event(["sequence": 1, "kind": "heartbeat"])).routeID, "route-a", "心跳只确认当前基线")
        XCTAssertThrowsError(try timeline.apply(event(["sequence": 3, "kind": "heartbeat"])), "发现缺口必须重新读取")
        XCTAssertNil(timeline.project, "缺口后不能把旧运行状态当实时")
        XCTAssertThrowsError(try timeline.apply(delta), "没有基线不能直接应用后续delta")
        XCTAssertEqual(try timeline.apply(event(["sequence": 0, "kind": "snapshot", "project": project], stream: "stream-b")).cards?.count, 2, "重连必须从新全量恢复")
        XCTAssertThrowsError(try timeline.apply(event(["sequence": 1, "kind": "heartbeat"], stream: "stream-b", workspace: "project-b")), "相同序号不能跨项目")
        XCTAssertNil(timeline.project, "来源不一致时清除可操作状态")
    }
}

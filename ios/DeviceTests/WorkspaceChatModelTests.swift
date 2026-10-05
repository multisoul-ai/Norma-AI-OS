import Foundation
import Testing
import XCTest
@testable import Norma
@testable import NormaCore

/// 场景：指定卡片的完整群聊草稿与来源想法跨重建保持，发送只生成工作空间消息。
/// 数据构造：缓存空间 A/B；草稿精确绑定 API 区域、definition 2、卡片8、运行r与会话s；单条想法idea。
/// 执行过程：保存目录 → 保存绑定草稿 → 重建 Model → 发送 → 读取 outbox 与线上消息字段。
/// 预期结果：目标始终是 API/8；A 消息仅一条；B 没有消息；不创建旧工作线程。
final class WorkspaceChatModelTests: XCTestCase {
@MainActor func testWorkspaceDraftBindsExactCardAndCreatesNoThread() async throws {
    let directory = URL.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let store = WorkspaceChatStore(databaseURL: directory.appendingPathComponent("chat.sqlite"))
    let catalog = try JSONDecoder().decode(WorkspaceCatalog.self, from: Data(#"{"mac_id":"mac","workspaces":[{"id":"a","name":"A","revision":1,"projects":[]},{"id":"b","name":"B","revision":1,"projects":[]}],"available_projects":[]}"#.utf8))
    try await store.saveCatalog(catalog)
    let first = WorkspaceChatModel(store: store)
    try await first.load(macID: "mac")
    let target = WorkspaceAgentTarget(areaID: "api", definitionID: 2, nodeID: 8, runID: "r", sessionID: "s")
    first.updateDraft(.init(text: "请执行", intent: .execute, target: target, ideaID: "idea"), workspaceID: "a")
    await first.flushDrafts()
    let second = WorkspaceChatModel(store: store)
    try await second.load(macID: "mac")
    #expect(second.drafts["a"]?.target == target, "重建必须保留具体区域和卡片")
    #expect(await second.send(workspaceID: "a", intent: .execute, target: target, ideaID: "idea"), "完整消息应进入持久待发送队列")
    let entries = try await store.outbox(macID: "mac", workspaceID: "a")
    #expect(entries.count == 1, "一次发送只保存一个稳定操作")
    #expect(entries[0].message.target?.nodeID == 8, "不能派给同名 iOS 卡片7")
    #expect(entries[0].message.target?.areaID == "api", "目标项目不能跟随桌面选择")
    #expect(try await store.outbox(macID: "mac", workspaceID: "b").isEmpty, "另一空间必须保持空白")
    #expect(try await store.draft(macID: "mac", workspaceID: "a").isEmpty, "清草稿与入队必须在同一事务完成")
    let fields = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(entries[0].message)) as? [String: Any])
    #expect(fields["desktop_workspace_id"] as? String == "a", "线上消息必须直接归属桌面工作空间")
    #expect(fields["thread_id"] == nil, "新流程不能再依赖独立线程身份")
}

}

import Foundation
import Testing
@testable import NormaCore

/// 场景：待发送群聊消息跨重启保留，目标不会随桌面选择变化。
/// 数据构造：单条消息，空间 A、项目区域 P、卡片 7；稳定操作 ID 为 UUID。
/// 执行过程：追加 → 重开存储 → 读取待发送 → 保存草稿 → 再次读取。
/// 预期结果：操作 ID、空间和卡片保持不变；不存在空间 B 的消息。
@Test func workspaceOutboxSurvivesRestart() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let url = directory.appendingPathComponent("chat.sqlite")
    let store = WorkspaceChatStore(databaseURL: url)
    let message = WorkspaceChatMessage(workspaceID: "space-a", text: "请检查首屏", intent: .execute,
                                       target: WorkspaceAgentTarget(areaID: "area-p", definitionID: 1, nodeID: 7, runID: nil, sessionID: nil))
    try await store.enqueue(message, macID: "mac-test")
    let reopened = WorkspaceChatStore(databaseURL: url)
    let pending = try await reopened.pending(macID: "mac-test")
    #expect(pending.count == 1, "重启不能丢失待发送消息")
    #expect(pending.first?.message.operationID == message.operationID, "重启或换连接不能生成新操作ID")
    #expect(pending.first?.message.target?.nodeID == 7, "目标必须仍为指定卡片7")
    #expect(pending.first?.message.workspaceID != "space-b", "不能按当前空间改派")
    try await reopened.saveDraft("继续讨论", macID: "mac-test", workspaceID: "space-a")
    #expect(try await reopened.draft(macID: "mac-test", workspaceID: "space-a") == "继续讨论", "草稿须长期保留")
}

/// 场景：草稿保留完整目标和想法来源，重连事件不能串入其它工作空间。
/// 数据构造：空间 A、卡片 7、原任务 task-1；一条空间 B 的伪回传。
/// 执行过程：保存草稿 → 重开读取 → 接收 B 的事件 → 检查 A 游标。
/// 预期结果：原目标和来源存在；错误事件被拒绝，游标未前进。
@Test func workspaceDraftAndEventScopeAreDurable() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let url = directory.appendingPathComponent("chat.sqlite")
    let store = WorkspaceChatStore(databaseURL: url)
    let target = WorkspaceAgentTarget(areaID: "p", definitionID: 1, nodeID: 7, runID: "r", sessionID: "s", taskRef: "task-1")
    let draft = WorkspaceChatDraft(text: "继续", intent: .execute, target: target, ideaID: "idea-1")
    try await store.saveDraft(draft, macID: "mac", workspaceID: "a")
    let reopened = WorkspaceChatStore(databaseURL: url)
    #expect(try await reopened.draftState(macID: "mac", workspaceID: "a") == draft, "返回页面或重启须保持目标、模式和来源")
    let data = Data(#"{"mac_id":"mac","desktop_workspace_id":"b","cursor":1,"device_id":"phone","operation_id":"op","kind":"received","payload":{"text":"wrong"}}"#.utf8)
    let event = try JSONDecoder().decode(WorkspaceChatEvent.self, from: data)
    await #expect(throws: WorkspaceChatError.invalid) { try await reopened.applyEvents([event], macID: "mac", workspaceID: "a", after: 0) }
    #expect(try await reopened.cursor(macID: "mac", workspaceID: "a") == 0, "串空间的事件不能推进游标")
    #expect(try await reopened.events(macID: "mac", workspaceID: "a").isEmpty, "错误消息不能进入长期历史")
}

/// 场景：Mac 删除工作空间后，手机保留消息和草稿，但不会自动恢复已删除配置。
/// 数据构造：目录先包含 A 与 B，随后只包含 B；A 留有一条消息和未提交草稿。
/// 执行过程：保存旧目录 → 记录 A → 保存新目录 → 重开存储读取目录、消息和草稿。
/// 预期结果：当前目录只有 B；A 的原消息和未提交草稿保留。
@Test func removedWorkspaceRetainsOutboxAndDraft() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let url = directory.appendingPathComponent("chat.sqlite")
    let store = WorkspaceChatStore(databaseURL: url)
    let decoder = JSONDecoder()
    let before = try decoder.decode(WorkspaceCatalog.self, from: Data(#"{"mac_id":"mac","workspaces":[{"id":"a","name":"A","revision":1,"projects":[]},{"id":"b","name":"B","revision":1,"projects":[]}],"available_projects":[]}"#.utf8))
    let after = try decoder.decode(WorkspaceCatalog.self, from: Data(#"{"mac_id":"mac","workspaces":[{"id":"b","name":"B","revision":2,"projects":[]}],"available_projects":[]}"#.utf8))
    try await store.saveCatalog(before)
    try await store.enqueue(.init(workspaceID: "a", text: "历史消息"), macID: "mac")
    try await store.saveDraft("未提交文字", macID: "mac", workspaceID: "a")
    try await store.saveCatalog(after)
    let reopened = WorkspaceChatStore(databaseURL: url)
    #expect(try await reopened.catalog(macID: "mac")?.workspaces.map(\.id) == ["b"], "已删除空间不能成为可执行目录")
    #expect(try await reopened.outbox(macID: "mac", workspaceID: "a").first?.message.text == "历史消息", "删除配置不能删除聊天原文")
    #expect(try await reopened.draft(macID: "mac", workspaceID: "a") == "未提交文字", "历史空间的未提交输入必须保留")
}

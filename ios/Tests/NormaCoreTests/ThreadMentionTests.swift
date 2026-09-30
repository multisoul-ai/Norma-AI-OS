import XCTest
import SQLite3
@testable import NormaCore

@MainActor final class ThreadMentionTests: XCTestCase {
    /// 直接呼叫冻结当前线程共享上下文，并原子生成具名任务；普通消息不派工。
    /// 数据构造：2 个 Workspace、主空间 2 个线程、2 位不同名成员；1 个本线程目标、1 个跨线程共同事实。
    /// 执行过程：保存普通消息/事实/其他线程私有内容 → 发送双成员提及 → 重启读取 → 同消息重试。
    /// 预期结果：恰有 2 个原操作，含本线程和共享事实；不含其他线程目标/原文或另一 Workspace 内容。
    func testMentionDispatchKeepsContextScopedAndRetryStable() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspace.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "移动端体验")
        workspace.projects = [project]; workspace.agents = [developer, designer]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let sibling = try await store.createThread(workspaceID: workspace.id, title: "另一个线程")
        let other = try await store.create(name: "独立空间")
        let foreign = try await store.createThread(workspaceID: other.id, title: "其他工作")
        _ = try await store.saveFact(threadID: thread.id, kind: .goal, text: "本线程首屏目标")
        _ = try await store.saveFact(threadID: sibling.id, kind: .goal, text: "不能共享的另一目标")
        _ = try await store.saveFact(threadID: sibling.id, kind: .fact, text: "共同事实：浅色 iOS")
        _ = try await store.saveFact(threadID: foreign.id, kind: .fact, text: "外部工作空间内容")
        _ = try await store.postThreadMessage(threadID: thread.id, id: UUID().uuidString, text: "本线程的最近讨论")
        _ = try await store.appendMessage(threadID: sibling.id, id: UUID().uuidString, text: "不能自动注入的其他线程原文")
        let initial = try await store.pendingThreadTasks()
        XCTAssertTrue(initial.isEmpty, "普通讨论不能派工")
        let id = UUID().uuidString
        let message = try await store.postThreadMessage(threadID: thread.id, id: id, text: "@Codex 核对状态，@Design 调整信息层级")
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let tasks = try await restarted.threadTasks(threadID: thread.id)
        XCTAssertEqual(tasks.count, 2, "每个明确呼叫的成员只生成一个操作")
        XCTAssertEqual(tasks.first?.task.agentDefinitionID, 2, "第一项交给选定开发成员")
        XCTAssertEqual(tasks.last?.task.agentDefinitionID, 3, "第二项交给设计成员")
        XCTAssertEqual(tasks.first?.task.sourceMessageID, id, "两项都必须绑定用户的原始来源")
        XCTAssertEqual(tasks.last?.task.sourceMessageID, id, "第二个任务不能串到另一条消息")
        let context = try XCTUnwrap(tasks.first?.task.context)
        let snapshot = try JSONDecoder().decode(ThreadTaskContext.self, from: Data(context.utf8))
        XCTAssertEqual(snapshot.threadID, thread.id, "任务详情应回看原线程的冻结上下文")
        XCTAssertEqual(snapshot.recentMessages.map(\.text), ["本线程的最近讨论"], "共享历史只能包含发送前本线程的消息")
        XCTAssertTrue(context.contains("本线程首屏目标"), "上下文含当前目标")
        XCTAssertTrue(context.contains("共同事实：浅色 iOS"), "共享事实可来自同工作空间另一线程")
        XCTAssertTrue(context.contains("本线程的最近讨论"), "上下文含当前线程最近记录")
        XCTAssertFalse(context.contains("不能共享的另一目标"), "不能混入另一线程目标")
        XCTAssertFalse(context.contains("不能自动注入的其他线程原文"), "其他线程原文只能按需获取")
        XCTAssertFalse(context.contains("外部工作空间内容"), "不能泄漏另一工作空间内容")
        XCTAssertEqual(message.state, .pending, "本机保存不能显示收到 Mac 回执")
        _ = try await restarted.postThreadMessage(threadID: thread.id, id: id, text: message.text)
        let repeated = try await restarted.threadTasks(threadID: thread.id)
        XCTAssertEqual(repeated, tasks, "重复保存同消息不能重新派工或更新上下文")
    }

    /// 同名成员需保留明确选择；删除提及解除目标，未知提及/超大简报不吞掉草稿。
    /// 数据构造：同一项目两位同名 Codex；1 条绑定 definition=3 的草稿；40KiB 事实超过任务+上下文 32KiB。
    /// 执行过程：歧义失败 → 保存选择/重启 → 精确派工 → 去掉提及 → 未知提及 → 超限简报发送。
    /// 预期结果：只有选中的定义收到任务；普通讨论不派工；所有失败保留输入且不插入半条消息。
    func testDraftMentionIdentityAndFailurePreservation() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspace.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        let sameName = WorkspaceAgentReference(machineID: mac, projectID: "ios", definitionID: 3, name: "Codex", profile: "Claude")
        var workspace = try await store.create(name: "同名成员")
        workspace.projects = [project]; workspace.agents = [developer, sameName]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "具体目标")
        do { _ = try await store.postThreadMessage(threadID: thread.id, id: UUID().uuidString, text: "@Codex 检查"); XCTFail("同名不应猜选") }
        catch { XCTAssertTrue(error is ThreadMentionError, "需明确要求从选择器确定成员") }
        let draft = WorkThreadDraft(text: "@Codex 检查", mentions: [sameName])
        try await store.saveMessageDraft(draft, threadID: thread.id)
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let drafts = try await restarted.messageDrafts()
        XCTAssertEqual(drafts[thread.id]?.mentions, [sameName], "草稿重启后保留定义身份，不按名字重新猜测")
        _ = try await restarted.postThreadMessage(threadID: thread.id, id: UUID().uuidString, text: draft.text,
            selected: drafts[thread.id]?.mentions ?? [], clearingDraftRevision: draft.revision)
        _ = try await restarted.postThreadMessage(threadID: thread.id, id: UUID().uuidString, text: "仅讨论；邮箱 dev@example.com",
            selected: [sameName])
        let tasks = try await restarted.threadTasks(threadID: thread.id)
        XCTAssertEqual(tasks.count, 1, "删提及后即使有旧选择，也不能暗中继续派工")
        XCTAssertEqual(tasks.first?.task.agentDefinitionID, 3, "选择第二位同名成员必须保持其具体身份")
        XCTAssertNotEqual(tasks.first?.task.agentDefinitionID, 2, "不得改派给名字相同的第一位")
        let unknown = WorkThreadDraft(text: "@不存在 检查")
        try await restarted.saveMessageDraft(unknown, threadID: thread.id)
        do { _ = try await restarted.postThreadMessage(threadID: thread.id, id: UUID().uuidString, text: unknown.text, clearingDraftRevision: unknown.revision); XCTFail("未知提及不能静默变成留言") }
        catch { XCTAssertTrue(error is ThreadMentionError, "未知目标要有明确失败") }
        let unknownDrafts = try await restarted.messageDrafts()
        XCTAssertEqual(unknownDrafts[thread.id], unknown, "未知目标不能清空输入")
        // 40 * 1024 ASCII bytes > the task + context ceiling of 32 * 1024 bytes.
        _ = try await restarted.saveFact(threadID: thread.id, kind: .fact, text: String(repeating: "x", count: 40 * 1024))
        try await restarted.saveMessageDraft(draft, threadID: thread.id)
        do { _ = try await restarted.postThreadMessage(threadID: thread.id, id: UUID().uuidString, text: draft.text,
            selected: [sameName], clearingDraftRevision: draft.revision); XCTFail("不能静默截断已确认事实") }
        catch ThreadMentionError.contextTooLarge { }
        let finalDrafts = try await restarted.messageDrafts()
        let messages = try await restarted.messages(threadID: thread.id)
        XCTAssertEqual(finalDrafts[thread.id], draft, "上下文无法发送必须保留原草稿")
        XCTAssertEqual(messages.count, 2, "失败不得留下只有消息没有派工的半条记录")
        XCTAssertFalse(messages.contains { $0.text == unknown.text }, "未知目标的失败输入不能冒充已发送消息")
    }

    /// 损坏的历史范围不能经上下文快照发送给 Agent。
    /// 数据构造：2 个 Workspace，当前线程 1 条历史的 workspace_id 被改为另一空间；1 条待发送草稿。
    /// 执行过程：写入正常历史 → 注入范围损坏 → 呼叫成员 → 重开读取草稿和任务。
    /// 预期结果：明确拒绝保存；草稿保留，无新任务，错误范围的历史不能进入派工上下文。
    func testDamagedHistoryCannotEnterTaskContext() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspace.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "当前空间")
        workspace.projects = [project]; workspace.agents = [developer]
        try await store.save(workspace)
        let other = try await store.create(name: "其他空间")
        let thread = try await store.createThread(workspaceID: workspace.id, title: "目标")
        _ = try await store.appendMessage(threadID: thread.id, id: UUID().uuidString, text: "范围错误的历史")
        let draft = WorkThreadDraft(text: "@Codex 检查")
        try await store.saveMessageDraft(draft, threadID: thread.id)
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(path.path, &db), SQLITE_OK, "打开隔离测试库")
        defer { sqlite3_close(db) }
        XCTAssertEqual(sqlite3_exec(db, "UPDATE work_messages SET workspace_id='\(other.id)'", nil, nil, nil), SQLITE_OK,
                       "仅注入测试历史的错误范围")
        do {
            _ = try await store.postThreadMessage(threadID: thread.id, id: UUID().uuidString, text: draft.text, clearingDraftRevision: draft.revision)
            XCTFail("不得将损坏范围的历史放进派工上下文")
        } catch { XCTAssertTrue(error is WorkThreadError, "持久化损坏应明确拒绝") }
        let reopened = CollaborationWorkspaceStore(databaseURL: path)
        let drafts = try await reopened.messageDrafts()
        let tasks = try await reopened.threadTasks(threadID: thread.id)
        XCTAssertEqual(drafts[thread.id], draft, "失败不能清除原草稿")
        XCTAssertTrue(tasks.isEmpty, "不能留下携带损坏上下文的任务")
    }

    private let mac = "mac-" + String(repeating: "a", count: 64)
    private var project: WorkspaceProjectReference { .init(machineID: mac, projectID: "ios", name: "iOS App", machineName: "Mac") }
    private var developer: WorkspaceAgentReference { .init(machineID: mac, projectID: "ios", definitionID: 2, name: "Codex", profile: "Codex") }
    private var designer: WorkspaceAgentReference { .init(machineID: mac, projectID: "ios", definitionID: 3, name: "Design", profile: "Claude") }
}

import XCTest
@testable import NormaCore

@MainActor final class ThreadPlanTests: XCTestCase {
    // Mac安装证书SHA-256：32字节 × 每字节2位hex = 64位指纹。
    private let mac = "mac-" + String(repeating: "a", count: 64)
    private var project: WorkspaceProjectReference {
        .init(machineID: mac, projectID: "ios", name: "iOS", machineName: "测试 Mac")
    }
    private var agent: WorkspaceAgentReference {
        .init(machineID: mac, projectID: "ios", definitionID: 2, name: "Alice", profile: "Codex")
    }

    /// Norma 候选完整落盘才生成任务；重启和重复响应不重复派工。
    /// 数据构造：1 空间、2 线程、1 成员；原草稿1份、云端完整回复1份、候选任务1项。
    /// 执行过程：冻结当前上下文→检查原草稿和空任务→应用完整响应→重开→重复应用。
    /// 预期结果：仅用户与Norma两条消息、1个原操作；无其他线程原文，无Mac接收回执。
    func testPlanAppliesAtomicallyAndSurvivesRestart() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "移动端体验")
        workspace.projects = [project]; workspace.agents = [agent]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let other = try await store.createThread(workspaceID: workspace.id, title: "另一事项")
        _ = try await store.appendMessage(threadID: other.id, id: UUID().uuidString, text: "其他线程私聊不应混入")
        _ = try await store.saveFact(threadID: other.id, kind: .fact, text: "共同事实：浅色 iOS")
        let draft = WorkThreadDraft(text: "@Norma 请让 @Alice 检查首屏", mentions: [agent])
        try await store.saveMessageDraft(draft, threadID: thread.id)
        let prepared = try await store.prepareNormaTurn(threadID: thread.id, messageID: UUID().uuidString,
            text: draft.text, selected: [agent], clearingDraftRevision: draft.revision)
        let before = try await store.messages(threadID: thread.id)
        let pendingBefore = try await store.pendingThreadTasks()
        let savedDraft = try await store.messageDrafts()[thread.id]
        XCTAssertTrue(before.isEmpty, "模型结果到达前不能保存虚构的Norma回复")
        XCTAssertTrue(pendingBefore.isEmpty, "冻结快照不能触发派工")
        XCTAssertEqual(savedDraft, draft, "等待模型时草稿必须留在手机")
        XCTAssertTrue(prepared.request.context.contains("共同事实：浅色 iOS"), "应共享空间内确认的事实")
        XCTAssertFalse(prepared.request.context.contains("其他线程私聊不应混入"), "不能把另一个线程原文带给模型")
        XCTAssertTrue(prepared.request.context.contains(agent.id), "混合提及必须给出选中的稳定成员ID")
        let response = ThreadPlanResponse(turnID: prepared.request.turnID, workspaceID: workspace.id,
            threadID: thread.id, messageID: prepared.request.messageID, reply: "由 iOS Alice 检查首屏。",
            assignments: [.init(targetID: agent.id, task: "检查首屏导航")])
        let user = try await store.applyNormaTurn(response, prepared: prepared)
        let reopened = CollaborationWorkspaceStore(databaseURL: path)
        let messages = try await reopened.messages(threadID: thread.id)
        let tasks = try await reopened.threadTasks(threadID: thread.id)
        XCTAssertEqual(messages.map(\.author), ["user", "norma"], "用户原文与协调者回复必须同事务持久保存")
        XCTAssertEqual(messages.last?.text, "由 iOS Alice 检查首屏。", "Norma完整回复应长期保留")
        XCTAssertEqual(user.state, .pending, "保存手机不能冒充送达Mac")
        XCTAssertEqual(tasks.count, 1, "仅1个合法候选应产生1项任务")
        XCTAssertEqual(tasks.first?.task.sourceMessageID, user.id, "任务须绑定用户原始指令")
        XCTAssertEqual(tasks.first?.task.agentDefinitionID, 2, "应派给当前选择的Alice定义")
        XCTAssertNil(tasks.first?.receipt, "推理候选不是Mac执行回执")
        let drafts = try await reopened.messageDrafts()
        XCTAssertNil(drafts[thread.id], "只在整个事务成功后清空原草稿")
        let replay = try await reopened.applyNormaTurn(response, prepared: prepared)
        let after = try await reopened.threadTasks(threadID: thread.id)
        let history = try await reopened.messages(threadID: thread.id)
        XCTAssertEqual(replay.id, user.id, "重复完整响应应返回原消息")
        XCTAssertEqual(after, tasks, "重启后重复响应不能改变操作身份或创建新任务")
        XCTAssertEqual(history.count, 2, "重复响应不能复制协调者回复")
        let outbox = try await reopened.pendingThreadDeliveries()
        XCTAssertFalse(outbox.contains { $0.messageID == response.turnID }, "Norma回复不能冒充用户消息进入Mac待发队列")
    }

    /// 等待期间成员变化须整轮回滚；新草稿不被旧回合清除，已保存结果不受后续归档影响。
    /// 数据构造：同一项目2位Agent、2项候选；先移除第二位，使第一项可写而第二项被拒绝。
    /// 执行过程：准备→移除第二目标→应用失败→重开检查→恢复成员并保存新草稿→应用→归档并重放。
    /// 预期结果：失败零消息/零任务且旧草稿仍在；成功恰2项、新草稿仍在；重复不新增或改写结果。
    func testMembershipChangeRollsBackAllAndPreservesNewDraft() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspace.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        let second = WorkspaceAgentReference(machineID: mac, projectID: "ios", definitionID: 3, name: "Bob", profile: "Codex")
        var workspace = try await store.create(name: "原子派工")
        workspace.projects = [project]; workspace.agents = [agent, second]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "导航")
        let original = WorkThreadDraft(text: "@Norma 请安排开发与检查")
        try await store.saveMessageDraft(original, threadID: thread.id)
        let prepared = try await store.prepareNormaTurn(threadID: thread.id, messageID: UUID().uuidString,
            text: original.text, clearingDraftRevision: original.revision)
        let response = ThreadPlanResponse(turnID: prepared.request.turnID, workspaceID: workspace.id,
            threadID: thread.id, messageID: prepared.request.messageID, reply: "安排 Alice 开发、Bob 检查。",
            assignments: [.init(targetID: agent.id, task: "开发导航"), .init(targetID: second.id, task: "检查导航")])
        workspace.agents = [agent]; try await store.save(workspace)
        do {
            _ = try await store.applyNormaTurn(response, prepared: prepared)
            XCTFail("第二目标移除后必须整轮拒绝")
        } catch ThreadDeliveryError.targetChanged { }
        let reopened = CollaborationWorkspaceStore(databaseURL: path)
        let emptyMessages = try await reopened.messages(threadID: thread.id)
        let emptyTasks = try await reopened.threadTasks(threadID: thread.id)
        let retained = try await reopened.messageDrafts()[thread.id]
        XCTAssertTrue(emptyMessages.isEmpty, "回滚必须撤回用户消息及Norma回复")
        XCTAssertTrue(emptyTasks.isEmpty, "第一项成功写入也必须随第二项失败一起回滚")
        XCTAssertEqual(retained, original, "失败不能清除已保存草稿")
        workspace.agents = [agent, second]; try await store.save(workspace)
        let newDraft = WorkThreadDraft(text: "随后再检查深色模式")
        try await store.saveMessageDraft(newDraft, threadID: thread.id)
        _ = try await store.applyNormaTurn(response, prepared: prepared)
        let kept = try await reopened.messageDrafts()[thread.id]
        let tasks = try await reopened.threadTasks(threadID: thread.id)
        XCTAssertEqual(kept, newDraft, "等待模型期间新编辑的文字不能被旧回合吞掉")
        XCTAssertEqual(tasks.count, 2, "全部目标恢复后才允许两项一起保存")
        XCTAssertEqual(tasks[0].task.agentDefinitionID, 2, "第一项仍属于Alice")
        XCTAssertEqual(tasks[1].task.agentDefinitionID, 3, "第二项仍属于Bob")
        workspace.archived = true; try await store.save(workspace)
        _ = try await reopened.applyNormaTurn(response, prepared: prepared)
        let replayed = try await reopened.threadTasks(threadID: thread.id)
        XCTAssertEqual(replayed, tasks, "已完成的应用重放只读取原记录，不因归档重新解释")
        let changed = ThreadPlanResponse(turnID: response.turnID, workspaceID: workspace.id, threadID: thread.id,
            messageID: response.messageID, reply: "变造已保存的回复", assignments: response.assignments)
        do {
            _ = try await store.applyNormaTurn(changed, prepared: prepared)
            XCTFail("同一回合不同结果不能覆盖原记录")
        } catch WorkThreadError.conflict { }
        let finalHistory = try await reopened.messages(threadID: thread.id)
        XCTAssertEqual(finalHistory.last?.text, response.reply, "冲突结果不能改写Norma的原始回复")
        XCTAssertFalse(finalHistory.contains { $0.text == changed.reply }, "不能保存变造的第二份结果")
    }

    /// 用户选中的同名成员不能被模型换成另一位。
    /// 数据构造：同项目2位Alice，definition=2与3；用户明确选择3，模型误选2；另有Bob=4。
    /// 执行过程：保存具名草稿→冻结选择→拒绝误选→验证草稿→应用选中Alice与Bob。
    /// 预期结果：误选整体拒绝、无任务/回复，原草稿仍在；合法分工允许不同名成员协助。
    func testPlanCannotReplaceExplicitSameNameSelection() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspace.sqlite"))
        let selected = WorkspaceAgentReference(machineID: mac, projectID: "ios", definitionID: 3, name: "Alice", profile: "Codex")
        let bob = WorkspaceAgentReference(machineID: mac, projectID: "ios", definitionID: 4, name: "Bob", profile: "Codex")
        var workspace = try await store.create(name: "精确选择")
        workspace.projects = [project]; workspace.agents = [agent, selected, bob]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let draft = WorkThreadDraft(text: "@Norma 请让 @Alice 检查", mentions: [selected])
        try await store.saveMessageDraft(draft, threadID: thread.id)
        let prepared = try await store.prepareNormaTurn(threadID: thread.id, messageID: UUID().uuidString,
            text: draft.text, selected: [selected], clearingDraftRevision: draft.revision)
        let wrong = ThreadPlanResponse(turnID: prepared.request.turnID, workspaceID: workspace.id,
            threadID: thread.id, messageID: prepared.request.messageID, reply: "安排Alice检查。",
            assignments: [.init(targetID: agent.id, task: "检查首屏")])
        do {
            _ = try await store.applyNormaTurn(wrong, prepared: prepared)
            XCTFail("模型不得改派给未选中的同名成员")
        } catch CloudTextError.invalidResponse { }
        let tasks = try await store.threadTasks(threadID: thread.id)
        let messages = try await store.messages(threadID: thread.id)
        let kept = try await store.messageDrafts()[thread.id]
        XCTAssertTrue(tasks.isEmpty, "错误目标不能进入待发队列")
        XCTAssertTrue(messages.isEmpty, "错误候选不能留下协调者成功回复")
        XCTAssertEqual(kept, draft, "原输入和definition=3选择必须保留")
        let valid = ThreadPlanResponse(turnID: prepared.request.turnID, workspaceID: workspace.id,
            threadID: thread.id, messageID: prepared.request.messageID, reply: "由选中的Alice检查，Bob复核。",
            assignments: [.init(targetID: selected.id, task: "检查首屏"), .init(targetID: bob.id, task: "复核首屏")])
        _ = try await store.applyNormaTurn(valid, prepared: prepared)
        let applied = try await store.threadTasks(threadID: thread.id)
        XCTAssertEqual(Set(applied.map(\.task.agentDefinitionID)), Set([Int64(3), Int64(4)]), "保留明确选择并允许不同名成员协助")
        XCTAssertFalse(applied.contains { $0.task.agentDefinitionID == 2 }, "未选中的同名成员始终不得收到任务")
    }
}

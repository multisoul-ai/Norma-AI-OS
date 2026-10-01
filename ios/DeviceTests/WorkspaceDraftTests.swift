import XCTest
@testable import Norma
@testable import NormaCore

final class WorkspaceDraftTests: XCTestCase {
    /// 同名 Agent 选择绑定具体项目，重启和移除正文提及不能悄悄使用旧目标。
    /// 数据构造：1 台 Mac、iOS/API 两个项目各有名为“开发”的成员；固定定义 ID 2/3 区分身份。
    /// 执行过程：选 iOS → 改选 API → flush/重建 Model → 删除提及 → 发送普通消息。
    /// 预期结果：只有最新选中成员；文本不重复；重启保持 API 身份；删除提及后无任务生成。
    @MainActor func testSameNameMentionSelectionPersistsExactProject() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspaces.sqlite"))
        let mac = "mac-" + String(repeating: "a", count: 64)
        let iosAgent = WorkspaceAgentReference(machineID: mac, projectID: "ios", definitionID: 2, name: "开发", profile: "Codex")
        let apiAgent = WorkspaceAgentReference(machineID: mac, projectID: "api", definitionID: 3, name: "开发", profile: "Codex")
        var workspace = try await store.create(name: "同名成员")
        workspace.projects = [.init(machineID: mac, projectID: "ios", name: "iOS", machineName: "Mac"),
                              .init(machineID: mac, projectID: "api", name: "API", machineName: "Mac")]
        workspace.agents = [iosAgent, apiAgent]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "检查")
        let model = WorkspaceLibraryModel(store: store)
        await model.load()
        model.mentionAgent(iosAgent, threadID: thread.id)
        XCTAssertEqual(model.messageDrafts[thread.id]?.mentions, [iosAgent], "第一次选择明确绑定 iOS 项目")
        model.mentionAgent(apiAgent, threadID: thread.id)
        try await model.flushMessageDrafts()
        let reopened = WorkspaceLibraryModel(store: store)
        await reopened.load()
        XCTAssertEqual(reopened.messageDrafts[thread.id]?.mentions, [apiAgent], "重启仍须显示 API 的具名目标")
        XCTAssertFalse(reopened.messageDrafts[thread.id]?.mentions?.contains(iosAgent) == true, "改选不能暗留同名旧成员")
        XCTAssertEqual(reopened.messageDrafts[thread.id]?.text, "@开发 ", "改选相同名称不重复插入提及")
        reopened.updateMessageDraft("先讨论", threadID: thread.id)
        XCTAssertEqual(reopened.messageDrafts[thread.id]?.mentions ?? [], [], "删除提及须同时移除选中身份；空集合按兼容格式保存为 nil")
        _ = try await reopened.sendMessage(threadID: thread.id, text: "先讨论")
        let tasks = try await store.threadTasks(threadID: thread.id)
        XCTAssertTrue(tasks.isEmpty, "删掉提及后的普通讨论不得触发旧成员派工")
    }

    /// 连续输入和页面重读不覆盖新草稿，发送期间修改又改回同文也不能被旧消息清空。
    /// 数据构造：隔离 SQLite 中 1 个 Workspace/线程，连续 2 次编辑，再同时发送和两次修改回原文。
    /// 执行过程：加载 → 连续编辑 → 重读 Workspace → flush → 重建 Model → 发消息并输入新草稿 → 重启读取。
    /// 预期结果：恢复最后一次输入；仅发送原文；新草稿保留，发送中的旧草稿不复活。
    @MainActor func testDraftEditsSurviveReloadAndSend() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspaces.sqlite"))
        let workspace = try await store.create(name: "移动端体验")
        let thread = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let model = WorkspaceLibraryModel(store: store)
        await model.load()
        model.updateMessageDraft("先梳理", threadID: thread.id)
        model.updateMessageDraft("先梳理首屏", threadID: thread.id)
        await model.load()
        try await model.flushMessageDrafts()
        let reopened = WorkspaceLibraryModel(store: store)
        await reopened.load()
        XCTAssertEqual(reopened.messageDrafts[thread.id]?.text, "先梳理首屏", "重读页面或重启必须保留最新编辑")
        let sendingRevision = reopened.messageDrafts[thread.id]?.revision
        let send = Task { @MainActor in
            // Both operations share MainActor: this child cannot run until sendMessage's first suspension,
            // after its synchronous prefix has captured the revision. No FIFO ordering between sibling tasks is assumed.
            let editing = Task { @MainActor in
                reopened.updateMessageDraft("再看导航", threadID: thread.id)
                reopened.updateMessageDraft("先梳理首屏", threadID: thread.id)
            }
            let message = try await reopened.sendMessage(threadID: thread.id, text: "先梳理首屏")
            await editing.value
            return message
        }
        _ = try await send.value
        try await reopened.flushMessageDrafts()
        let finalDrafts = try await store.messageDrafts()
        let messages = try await store.messages(threadID: thread.id)
        XCTAssertEqual(finalDrafts[thread.id]?.text, "先梳理首屏", "发送原文不能清空随后编辑的同文草稿")
        XCTAssertNotEqual(finalDrafts[thread.id]?.revision, sendingRevision, "相同原文的新版本应独立于发送版本")
        XCTAssertEqual(messages.map(\.text), ["先梳理首屏"], "只把明确发送的原文追加到历史")
        XCTAssertFalse(messages.contains { $0.text == "再看导航" }, "新草稿不得自动发送")
        XCTAssertTrue(reopened.draftErrors.isEmpty, "正常落盘后不得残留保存错误")
    }

    /// 超长草稿保留在原线程输入框，另一线程仍可落盘和发送。
    /// 数据构造：2 个线程；A 输入 100001 = 100000 上限 + 1 字；B 输入正常短句。
    /// 执行过程：编辑 A/B → 只 flush B → 发送 B → 检查两线程错误和历史。
    /// 预期结果：A 原文不截断且有明确超限提示；B 没有保存错误且只发送一条真实原文。
    @MainActor func testOversizedDraftDoesNotBlockOtherThread() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspaces.sqlite"))
        let workspace = try await store.create(name: "测试")
        let first = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let second = try await store.createThread(workspaceID: workspace.id, title: "导航")
        let model = WorkspaceLibraryModel(store: store)
        await model.load()
        model.updateMessageDraft(String(repeating: "字", count: 100_001), threadID: first.id)
        model.updateMessageDraft("先检查导航", threadID: second.id)
        try await model.flushMessageDrafts(threadID: second.id)
        XCTAssertEqual(model.messageDrafts[first.id]?.text.count, 100_001, "无效长草稿不能被截断或丢弃")
        XCTAssertTrue(model.draftErrors[first.id]?.contains("100,000") == true, "原线程必须明确说明超限原因")
        XCTAssertNil(model.draftErrors[second.id], "有效线程不继承其他线程保存失败")
        _ = try await model.sendMessage(threadID: second.id, text: "先检查导航")
        try await model.flushMessageDrafts(threadID: second.id)
        let messages = try await store.messages(threadID: second.id)
        let firstMessages = try await store.messages(threadID: first.id)
        XCTAssertEqual(messages.map(\.text), ["先检查导航"], "其他草稿超限不能阻塞正常发送")
        XCTAssertTrue(firstMessages.isEmpty, "超限草稿不得自动截断成为消息")
        XCTAssertNil(model.messageDrafts[second.id], "发送成功清理自己的草稿")
    }
    /// 追问对象在编辑与重启后保留；正文中的提及不能把原任务追问变成新派工。
    /// 数据构造：1个已签收原任务、1个空正文追问草稿，随后正文为含@Norma和@开发的问题。
    /// 执行过程：加载空草稿→输入→保存并重建Model→发送同时输入新草稿→重启读取。
    /// 预期结果：只新增一条来源和一条追问，没有新增任务；原任务ID固定，新revision草稿不被清空。
    @MainActor func testFollowupTargetSurvivesEditsAndRoutesBeforeMentions() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspaces.sqlite"))
        let parent = try await followupParent(store)
        let threadID = parent.task.threadID
        try await store.saveMessageDraft(.init(text: "", followupTaskID: parent.id), threadID: threadID)
        let model = WorkspaceLibraryModel(store: store)
        await model.load()
        let text = "@Norma @开发 请解释这次修改"
        model.updateMessageDraft(text, threadID: threadID)
        try await model.flushMessageDrafts()
        let reopened = WorkspaceLibraryModel(store: store)
        await reopened.load()
        XCTAssertEqual(reopened.messageDrafts[threadID]?.followupTaskID, parent.id, "编辑不能丢失原任务身份")
        let revision = reopened.messageDrafts[threadID]?.revision
        let editing = Task { @MainActor in reopened.updateMessageDraft("下一条问题", threadID: threadID) }
        let sent = try await reopened.sendMessage(threadID: threadID, text: text)
        await editing.value
        try await reopened.flushMessageDrafts()
        let followups = try await store.threadFollowups(threadID: threadID)
        let tasks = try await store.threadTasks(threadID: threadID)
        let drafts = try await store.messageDrafts()
        XCTAssertEqual(followups.count, 1, "明确追问只生成一条独立续聊操作")
        XCTAssertEqual(followups.first?.followup.parentOperationID, parent.id, "继续原任务而非依据提及重新派工")
        XCTAssertEqual(followups.first?.followup.sourceMessageID, sent.id, "追问必须绑定本次消息")
        XCTAssertEqual(tasks.map(\.id), [parent.id], "正文含提及也不得创建新任务")
        XCTAssertEqual(drafts[threadID]?.text, "下一条问题", "发送成功不能清除随后输入")
        XCTAssertNotEqual(drafts[threadID]?.revision, revision, "新草稿独立于已发送版本")
        XCTAssertEqual(drafts[threadID]?.followupTaskID, parent.id, "发送中继续编辑仍明确追问原任务")
        _ = try await reopened.sendMessage(threadID: threadID, text: "下一条问题")
        try await reopened.flushMessageDrafts()
        let final = try await store.messageDrafts()
        XCTAssertNil(final[threadID], "未再编辑时成功发送同时清除正文和回复对象")
        XCTAssertNil(reopened.messageDrafts[threadID], "内存与持久草稿必须一致")
    }

    /// 从任务详情选择追问保留原文，取消只去掉对象，错误线程不能换绑。
    /// 数据构造：1个已签收任务和原线程草稿，另一个线程为空；最后使用空正文再选择同一任务。
    /// 执行过程：选择→编辑→取消→跨线程选择失败→清空正文再选择→重启。
    /// 预期结果：草稿和提及逐字保留，目标可取消；错误线程无改动，空正文对象仍恢复。
    @MainActor func testSelectingAndCancellingFollowupPreservesDraft() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspaces.sqlite"))
        let parent = try await followupParent(store)
        let threadID = parent.task.threadID
        let other = try await store.createThread(workspaceID: parent.task.collaborationWorkspaceID, title: "另一个线程")
        let model = WorkspaceLibraryModel(store: store); await model.load()
        model.mentionAgent(try XCTUnwrap(parent.target), threadID: threadID)
        model.updateMessageDraft("@开发 已写好的问题", threadID: threadID)
        try await model.beginFollowup(operationID: parent.id, threadID: threadID)
        XCTAssertEqual(model.messageDrafts[threadID]?.text, "@开发 已写好的问题", "选对象不能替换或增加已有正文")
        XCTAssertEqual(model.messageDrafts[threadID]?.mentions, [parent.target!], "取消后应能恢复原有明确提及")
        model.cancelFollowup(threadID: threadID)
        XCTAssertNil(model.messageDrafts[threadID]?.followupTaskID, "取消须立即去掉对象")
        XCTAssertEqual(model.messageDrafts[threadID]?.text, "@开发 已写好的问题", "取消不能清空正文")
        do { try await model.beginFollowup(operationID: parent.id, threadID: other.id); XCTFail("不能跨线程选择原任务") }
        catch { XCTAssertTrue(error is WorkThreadError, "必须拒绝错误线程身份") }
        XCTAssertNil(model.messageDrafts[other.id], "错误线程不能留下半份草稿")
        model.updateMessageDraft("", threadID: threadID)
        try await model.beginFollowup(operationID: parent.id, threadID: threadID)
        let reopened = WorkspaceLibraryModel(store: store); await reopened.load()
        XCTAssertEqual(reopened.messageDrafts[threadID]?.followupTaskID, parent.id, "尚未输入文字时也要恢复对象")
        XCTAssertEqual(reopened.messageDrafts[threadID]?.text, "", "选对象不自动插入指令")
        reopened.cancelFollowup(threadID: threadID); try await reopened.flushMessageDrafts()
        let final = try await store.messageDrafts()
        XCTAssertNil(final[threadID], "取消空正文对象后不留下空草稿")
    }

    @MainActor private func followupParent(_ store: CollaborationWorkspaceStore) async throws -> ThreadTaskOutboxOperation {
        let mac = "mac-" + String(repeating: "a", count: 64)
        let agent = WorkspaceAgentReference(machineID: mac, projectID: "ios", definitionID: 2, name: "开发", profile: "Codex")
        var workspace = try await store.create(name: "追问草稿")
        workspace.projects = [.init(machineID: mac, projectID: "ios", name: "iOS", machineName: "Mac")]
        workspace.agents = [agent]; try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "交接")
        let task = try await store.appendTaskMessage(threadID: thread.id, messageID: UUID().uuidString,
            text: "改首屏", operationID: UUID().uuidString, target: agent, task: "改首屏", contextVersion: 1, context: "")
        let sourceOutbox = try await store.prepareThreadDelivery(messageID: task.task.sourceMessageID)
        let source = try XCTUnwrap(sourceOutbox.message)
        // cursor=1为首条事件，时间戳为固定有效秒数。明确是Store验收fixture，不冒充真实网络回执。
        try await store.applyThreadReceipt(.init(sourceMacID: mac, collaborationWorkspaceID: source.collaborationWorkspaceID,
            threadID: source.threadID, operationID: source.operationID, messageID: source.messageID, originDeviceID: source.originDeviceID,
            fingerprint: source.fingerprint(machineID: mac), receivedAtUnix: 1_790_000_000, eventID: mac + ":1", cursor: 1, state: "received"))
        let receipt = try ThreadTaskReceipt(binding: .init(sourceMacID: mac, collaborationWorkspaceID: workspace.id,
            threadID: thread.id, sourceOperationID: task.task.sourceOperationID, sourceMessageID: task.task.sourceMessageID,
            payload: String(decoding: JSONEncoder().encode(task.task), as: UTF8.self)), fingerprint: task.task.fingerprint(machineID: mac),
            receipt: .init(deviceID: "phone-a", operationID: task.id, workspaceID: "ios", routeID: "route-a", state: "succeeded",
                result: .init(status: "accepted", message: nil, nodeID: 2, taskID: 7, taskRef: "runtime-a:7"), error: nil))
        try await store.applyThreadTaskReceipt(receipt, deviceID: "phone-a")
        return try await store.threadTask(operationID: task.id)
    }

}

import XCTest
import SQLite3
@testable import NormaCore

@MainActor final class ThreadTaskOutboxTests: XCTestCase {
    /// Mac 拒绝原因跟随回执持久化，重启后仍可读，不因临时网络错误清理而消失。
    /// 数据构造：1 个线程/来源/任务；回执错误 agent_disabled；另构造未知码检查兼容回退。
    /// 执行过程：保存并签收来源 → 应用拒绝回执 → 重开数据库 → 读取拒绝说明和待发队列。
    /// 预期结果：明确显示成员停用原因；原错误码保留，不再排队；接收态不显示拒绝原因。
    func testRejectionReasonSurvivesRestartAndDoesNotRetry() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "拒绝反馈")
        workspace.projects = [project]; workspace.agents = [agent]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "检查")
        let messageID = UUID().uuidString
        let task = try await store.appendTaskMessage(threadID: thread.id, messageID: messageID, text: "@开发 检查",
            operationID: UUID().uuidString, target: agent, task: "检查", contextVersion: 1, context: "")
        try await signSource(store, messageID: messageID)
        try await store.applyThreadTaskReceipt(taskReceipt(task, state: "rejected", error: "agent_disabled"), deviceID: "phone-a")
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let saved = try await restarted.threadTasks(threadID: thread.id)
        let pending = try await restarted.pendingThreadTasks()
        XCTAssertEqual(saved.first?.receipt?.receipt.error, "agent_disabled", "持久回执须保留真实拒绝原因")
        XCTAssertEqual(saved.first?.receipt?.rejectionDescription, "这位 Agent 已在 Mac 上停用，请在电脑上检查成员设置。", "重启后卡片与详情须能读取具体原因")
        XCTAssertTrue(pending.isEmpty, "拒绝的原任务不能自动重派")
        XCTAssertNil(try taskReceipt(task, state: "accepted").rejectionDescription, "接收态不得显示旧拒绝原因")
        XCTAssertEqual(try taskReceipt(task, state: "rejected", error: "future_error").rejectionDescription,
                       "Mac 拒绝了这项任务，请在电脑上核对。", "未知错误码也要有可读说明，不能假装成功")
        XCTAssertEqual(try taskReceipt(task, state: "rejected", error: "route_expired").rejectionDescription,
                       MobileError.sessionChanged.errorDescription, "项目变化应复用既有准确说明")
    }

    /// 离线呼叫原子保存消息与派工，重启和成员改名不能换任务或上下文。
    /// 数据构造：1 个 Workspace、2 个线程、1 台 Mac/项目/Agent；上下文版本 3 为本次冻结版本。
    /// 执行过程：保存草稿 → 呼叫并落盘 → 重建 Store → 同操作重试 → 改正文/跨线程重试。
    /// 预期结果：只保留原消息和原任务；草稿仅在成功后清空，讨论线程无任务，冲突不能覆盖。
    func testOfflineTaskAndSourceSurviveRestartWithoutRetargeting() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "首屏")
        workspace.projects = [project]; workspace.agents = [agent]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "实现")
        let discussion = try await store.createThread(workspaceID: workspace.id, title: "讨论")
        let draft = WorkThreadDraft(text: "@开发 修复首屏导航")
        try await store.saveMessageDraft(draft, threadID: thread.id)
        let messageID = UUID().uuidString, operationID = UUID().uuidString
        let queued = try await store.appendTaskMessage(threadID: thread.id, messageID: messageID,
            text: draft.text, operationID: operationID, target: agent, task: "修复首屏导航",
            contextVersion: 3, context: "目标：简化导航", clearingDraftRevision: draft.revision)
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let tasks = try await restarted.threadTasks(threadID: thread.id)
        let messages = try await restarted.messages(threadID: thread.id)
        let drafts = try await restarted.messageDrafts()
        let otherTasks = try await restarted.threadTasks(threadID: discussion.id)
        XCTAssertEqual(tasks, [queued], "重启必须恢复原操作和冻结上下文")
        XCTAssertEqual(messages.map(\.id), [messageID], "消息与派工必须同时存在")
        XCTAssertEqual(messages.first?.state, .pending, "本机保存不能伪装 Mac 送达")
        XCTAssertNil(drafts[thread.id], "成功保存后才能移除匹配草稿")
        XCTAssertTrue(otherTasks.isEmpty, "独立讨论不能自动派工或看到别的线程任务")
        let retried = try await restarted.enqueueThreadTask(sourceMessageID: messageID, operationID: operationID,
            target: agent, task: "修复首屏导航", contextVersion: 3, context: "目标：简化导航")
        XCTAssertEqual(retried, queued, "同内容重试不能生成新任务 ID")
        do {
            _ = try await restarted.enqueueThreadTask(sourceMessageID: messageID, operationID: operationID,
                target: agent, task: "改服务端", contextVersion: 4, context: "不同上下文")
            XCTFail("同操作不能改正文和上下文")
        } catch { XCTAssertTrue(error is WorkThreadError, "冲突必须明确拒绝") }
        let other = try await restarted.appendMessage(threadID: discussion.id, id: UUID().uuidString, text: "另一件事")
        do {
            _ = try await restarted.enqueueThreadTask(sourceMessageID: other.id, operationID: operationID,
                target: agent, task: "修复首屏导航", contextVersion: 3, context: "目标：简化导航")
            XCTFail("原操作不能归到另一个线程")
        } catch { XCTAssertTrue(error is WorkThreadError, "跨线程复用操作必须拒绝") }
        let pending = try await restarted.pendingThreadTasks()
        XCTAssertEqual(pending, [queued], "冲突后仍只有原任务待发")
        XCTAssertEqual(queued.task.sourceMessageID, messageID, "任务来源固定为本次消息")
        XCTAssertEqual(queued.task.contextVersion, 3, "重试不能重新生成上下文版本")
        XCTAssertNil(queued.receipt, "没有 Mac 回执不能认为已接单")
    }

    /// 派工写失败必须回滚来源消息和草稿清理；合法保存后配置移除只阻止新发送。
    /// 数据构造：1 条草稿；触发器拒绝任务 INSERT，故消息/派工新增量均为 0。
    /// 执行过程：注入失败 → 发送 → 读取草稿 → 移除失败点重试 → 伪造来源签收 → 移除/恢复成员。
    /// 预期结果：失败不丢输入、不产生半次派工；未送达与成员移除时不能准备发送，成员改名仍沿用原身份。
    func testAtomicRollbackAndTargetChecks() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "原子保存")
        workspace.projects = [project]; workspace.agents = [agent]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "任务")
        let draft = WorkThreadDraft(text: "@开发 修导航")
        try await store.saveMessageDraft(draft, threadID: thread.id)
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(path.path, &db), SQLITE_OK, "打开隔离测试库")
        defer { sqlite3_close(db) }
        XCTAssertEqual(sqlite3_exec(db, "CREATE TRIGGER fail_task BEFORE INSERT ON thread_task_outbox BEGIN SELECT RAISE(ABORT,'test failure'); END;", nil, nil, nil), SQLITE_OK, "安装持久化失败点")
        let id = UUID().uuidString, messageID = UUID().uuidString
        do {
            _ = try await store.appendTaskMessage(threadID: thread.id, messageID: messageID, text: draft.text,
                operationID: id, target: agent, task: "修导航", contextVersion: 1, context: "", clearingDraftRevision: draft.revision)
            XCTFail("任务无法持久保存时必须失败")
        } catch { XCTAssertTrue(error is WorkThreadError, "本机写失败应明确报告") }
        let messages = try await store.messages(threadID: thread.id)
        let drafts = try await store.messageDrafts()
        let sources = try await store.pendingThreadDeliveries()
        XCTAssertTrue(messages.isEmpty, "原消息必须随任务一起回滚")
        XCTAssertTrue(sources.isEmpty, "不能剩下孤立消息待发操作")
        XCTAssertEqual(drafts[thread.id], draft, "草稿正文及 revision 必须保留")
        XCTAssertEqual(sqlite3_exec(db, "DROP TRIGGER fail_task", nil, nil, nil), SQLITE_OK, "恢复隔离库的写入")
        let queued = try await store.appendTaskMessage(threadID: thread.id, messageID: messageID, text: draft.text,
            operationID: id, target: agent, task: "修导航", contextVersion: 1, context: "", clearingDraftRevision: draft.revision)
        do { _ = try await store.prepareThreadTask(operationID: id, deviceID: "phone-a"); XCTFail("来源未签收不能派工") }
        catch { XCTAssertTrue(error is ThreadTaskError, "应明确提示来源尚未送达") }
        try await signSource(store, messageID: messageID)
        workspace.agents = []
        try await store.save(workspace)
        do { _ = try await store.prepareThreadTask(operationID: id, deviceID: "phone-a"); XCTFail("移除成员必须阻止新派工") }
        catch { XCTAssertTrue(error is ThreadDeliveryError, "不能悄悄改派") }
        workspace.agents = [.init(machineID: mac, projectID: "project-a", definitionID: 2, name: "新名字", profile: "Codex")]
        try await store.save(workspace)
        let prepared = try await store.prepareThreadTask(operationID: id, deviceID: "phone-a")
        XCTAssertEqual(prepared.task, queued.task, "改名后仍为原定义/上下文，不重新生成任务")
        XCTAssertEqual(prepared.deviceID, "phone-a", "首次发送前应持久绑定配对设备")
        do { _ = try await store.prepareThreadTask(operationID: id, deviceID: "phone-b"); XCTFail("更换配对身份不能重派原操作") }
        catch { XCTAssertTrue(error is MobileError, "配对身份变化应明确拒绝") }
        workspace.archived = true
        try await store.save(workspace)
        do { _ = try await store.prepareThreadTask(operationID: id, deviceID: "phone-a"); XCTFail("归档后不能继续新的发送") }
        catch { XCTAssertTrue(error is WorkThreadError, "归档应保留原任务并停止投递") }
    }

    /// 接单/派工回执单调更新且持久保存，迟到或冲突回执不能误报完成或触发重派。
    /// 数据构造：2 个任务；任务一 accepted→succeeded，任务二 outcome_unknown；task_ref 为 runtime-a:7。
    /// 执行过程：保存任务/来源回执 → 绑定设备 → 错设备回执 → 正确接单/派工 → 迟到接单 → 未知结果 → 重开。
    /// 预期结果：错误身份不写入；终态不回退；未知不在待派工队列；保留完整 task_ref，不产生完成状态。
    func testReceiptRecoveryIsMonotonicAndUnknownDoesNotRetry() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "回执")
        workspace.projects = [project]; workspace.agents = [agent]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "执行")
        let messageID = UUID().uuidString
        let first = try await store.appendTaskMessage(threadID: thread.id, messageID: messageID, text: "@开发 修导航",
            operationID: UUID().uuidString, target: agent, task: "修导航", contextVersion: 1, context: "")
        let second = try await store.enqueueThreadTask(sourceMessageID: messageID, operationID: UUID().uuidString,
            target: agent, task: "复查导航", contextVersion: 1, context: "")
        try await signSource(store, messageID: messageID)
        _ = try await store.prepareThreadTask(operationID: first.id, deviceID: "phone-a")
        let accepted = try taskReceipt(first, state: "accepted")
        do { try await store.applyThreadTaskReceipt(accepted, deviceID: "phone-b"); XCTFail("错误设备不能应用回执") }
        catch { XCTAssertTrue(error is MobileError, "已绑定身份不匹配必须拒绝") }
        let untouched = try await store.threadTasks(threadID: thread.id)
        XCTAssertNil(untouched.first?.receipt, "错误回执不能改变任务状态")
        try await store.applyThreadTaskReceipt(accepted, deviceID: "phone-a")
        let succeeded = try taskReceipt(first, state: "succeeded")
        try await store.applyThreadTaskReceipt(succeeded, deviceID: "phone-a")
        try await store.applyThreadTaskReceipt(accepted, deviceID: "phone-a")
        try await store.applyThreadTaskReceipt(succeeded, deviceID: "phone-a")
        do {
            try await store.applyThreadTaskReceipt(taskReceipt(first, state: "outcome_unknown"), deviceID: "phone-a")
            XCTFail("冲突终态不能覆盖原成功派工证据")
        } catch { XCTAssertTrue(error is ThreadTaskError, "冲突须报告并保留原回执") }
        try await store.applyThreadTaskReceipt(taskReceipt(second, state: "outcome_unknown"), deviceID: "phone-a")
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let saved = try await restarted.threadTasks(threadID: thread.id)
        let pending = try await restarted.pendingThreadTasks()
        XCTAssertEqual(saved.first?.receipt, succeeded, "重启必须保留原派工回执，不被迟到的接单覆盖")
        XCTAssertEqual(saved.first?.receipt?.receipt.result?.taskRef, "runtime-a:7", "持久任务引用必须完整保留")
        XCTAssertEqual(saved.first?.receipt?.receipt.result?.status, "accepted", "派工成功不能改成任务完成")
        XCTAssertEqual(saved.last?.receipt?.receipt.state, "outcome_unknown", "未知结果应长期可见而非自动重派")
        XCTAssertTrue(pending.isEmpty, "已派工与结果未知都不能自动再次执行")
    }

    /// 损坏记录读取不能自行生成新的来源操作，避免破坏可恢复的原始数据库。
    /// 数据构造：1 个任务及来源；仅删除隔离库中的消息待发行，模拟旧数据局部损坏。
    /// 执行过程：保存任务 → 注入损坏 → 记录数据库字节 → 用公开接口读取任务。
    /// 预期结果：读取明确失败；原始损坏数据字节不变，没有隐式修补或新操作 ID。
    func testReadingDamagedTaskDoesNotRewriteItsMissingSourceOperation() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "损坏保护")
        workspace.projects = [project]; workspace.agents = [agent]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "保留")
        let task = try await store.appendTaskMessage(threadID: thread.id, messageID: UUID().uuidString, text: "@开发 检查",
            operationID: UUID().uuidString, target: agent, task: "检查", contextVersion: 1, context: "")
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(path.path, &db), SQLITE_OK, "打开隔离库注入损坏")
        XCTAssertEqual(sqlite3_exec(db, "DELETE FROM thread_outbox", nil, nil, nil), SQLITE_OK, "仅移除测试来源操作")
        sqlite3_close(db)
        let before = try Data(contentsOf: path)
        do { _ = try await store.threadTask(operationID: task.id); XCTFail("损坏来源不能被当成新操作") }
        catch { XCTAssertTrue(error is WorkThreadError, "读取必须报告存储损坏") }
        XCTAssertTrue(try Data(contentsOf: path) == before, "失败读取不得改写原数据库或隐式生成来源 ID")
    }

    private func signSource(_ store: CollaborationWorkspaceStore, messageID: String) async throws {
        let operation = try await store.prepareThreadDelivery(messageID: messageID)
        let payload = try XCTUnwrap(operation.message)
        // 本机 fixture 的第一条接收事件 cursor=1；不声称这是真实网络签收。
        try await store.applyThreadReceipt(.init(sourceMacID: mac, collaborationWorkspaceID: payload.collaborationWorkspaceID,
            threadID: payload.threadID, operationID: operation.id, messageID: messageID, originDeviceID: payload.originDeviceID,
            fingerprint: payload.fingerprint(machineID: mac), receivedAtUnix: 1_790_000_000, eventID: mac + ":1", cursor: 1, state: "received"))
    }

    private func taskReceipt(_ operation: ThreadTaskOutboxOperation, state: String, error: String? = nil) throws -> ThreadTaskReceipt {
        let task = operation.task
        return try .init(binding: .init(sourceMacID: mac, collaborationWorkspaceID: task.collaborationWorkspaceID,
            threadID: task.threadID, sourceOperationID: task.sourceOperationID, sourceMessageID: task.sourceMessageID,
            payload: String(decoding: JSONEncoder().encode(task), as: UTF8.self)), fingerprint: task.fingerprint(machineID: mac),
            receipt: .init(deviceID: "phone-a", operationID: operation.id, workspaceID: task.projectID, routeID: "route-a",
                state: state, result: state == "succeeded" ? .init(status: "accepted", message: nil, nodeID: 2, taskID: 7, taskRef: "runtime-a:7") : nil,
                error: state == "outcome_unknown" ? "outcome_unknown" : error))
    }

    private let mac = "mac-" + String(repeating: "a", count: 64)
    private var project: WorkspaceProjectReference {
        .init(machineID: mac, projectID: "project-a", name: "iOS", machineName: "Mac")
    }
    private var agent: WorkspaceAgentReference {
        .init(machineID: mac, projectID: "project-a", definitionID: 2, name: "开发", profile: "Codex")
    }
}

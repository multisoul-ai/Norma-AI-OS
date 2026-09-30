import XCTest
import SQLite3
@testable import NormaCore

@MainActor final class ThreadExecutionTests: XCTestCase {
    /// 手机只保存实际确认的进展，重复/迟到回执及补充成果不制造新步骤。
    /// 数据构造：派工accepted，执行submitted(seq=1)、done(seq=2)，补成果seq=3；
    ///   同步基准为1_790_000_000秒，每次真实状态变化相隔10秒，重复轮询不计进展。
    /// 执行过程：接收→执行→重放→完成→补成果→迟到回执→重开读取。
    /// 预期结果：仅3条原任务进展，保留首次同步时刻，别的线程为空；未记录未来步骤。
    func testProgressSurvivesRestartWithoutDuplicateOrFabricatedSteps() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let (store, operation, other) = try await fixture(path)
        let start = Date(timeIntervalSince1970: 1_790_000_000)
        let accepted = try receipt(operation, dispatch: "accepted")
        let running = try receipt(operation, sequence: 1)
        let completed = try receipt(operation, sequence: 2, state: "done", text: "完成检查")
        let withDelivery = try receipt(operation, sequence: 3, state: "done", text: "完成检查", delivery: true)
        try await store.applyThreadTaskReceipt(accepted, deviceID: "phone-a", at: start)
        try await store.applyThreadTaskReceipt(running, deviceID: "phone-a", at: start.addingTimeInterval(10))
        try await store.applyThreadTaskReceipt(running, deviceID: "phone-a", at: start.addingTimeInterval(20))
        try await store.applyThreadTaskReceipt(completed, deviceID: "phone-a", at: start.addingTimeInterval(30))
        try await store.applyThreadTaskReceipt(withDelivery, deviceID: "phone-a", at: start.addingTimeInterval(40))
        try await store.applyThreadTaskReceipt(running, deviceID: "phone-a", at: start.addingTimeInterval(50))
        try await store.applyThreadTaskReceipt(accepted, deviceID: "phone-a", at: start.addingTimeInterval(60))
        let reopened = CollaborationWorkspaceStore(databaseURL: path)
        let saved = try await reopened.threadTask(operationID: operation.id)
        let progress = try XCTUnwrap(saved.progress, "本轮实际观测的进展必须长期保留")
        XCTAssertEqual(progress.count, 3, "重复、成果补齐与迟到状态不能插入新进展")
        XCTAssertEqual(progress[0].dispatchState, "accepted", "第一条仅代表Mac已接收")
        XCTAssertNil(progress[0].executionState, "接收回执不能冒充已执行")
        XCTAssertEqual(progress[0].observedAt, start, "同步时刻保留首次观测时间")
        XCTAssertEqual(progress[1].executionState, "submitted", "第二条保留真实执行中状态")
        XCTAssertEqual(progress[1].observedAt, start.addingTimeInterval(10), "重放不能改写同步时间")
        XCTAssertEqual(progress[2].executionState, "done", "第三条来自真实完成回执")
        XCTAssertEqual(progress[2].observedAt, start.addingTimeInterval(30), "补成果不应改变完成状态首次同步时间")
        XCTAssertFalse(progress.contains { $0.executionState == "queued" }, "没有观测到排队，不能补造排队节点")
        XCTAssertEqual(saved.receipt, withDelivery, "完整成果必须仍能保存，不能为去重丢弃新内容")
        let elsewhere = try await reopened.threadTasks(threadID: other.id)
        XCTAssertTrue(elsewhere.isEmpty, "进展不能创建另一个线程的任务")
        var legacy = try JSONSerialization.jsonObject(with: JSONEncoder().encode(saved)) as! [String: Any]
        legacy.removeValue(forKey: "progress")
        let old = try JSONDecoder().decode(ThreadTaskOutboxOperation.self, from: JSONSerialization.data(withJSONObject: legacy))
        XCTAssertNil(old.progress, "旧任务不能因缺少进展字段而不可读取或生成伪造历史")
        XCTAssertEqual(old.receipt, saved.receipt, "旧任务的实际成果和回执不能丢失")
    }

    /// 退回修改原子创建同线程消息和关联任务，重试与重启不复制，失败保留原意见。
    /// 数据构造：1个已完成成果、原Agent定义2、意见「把按钮改为蓝色」；另一个线程不得收到任务。
    /// 执行过程：保存意见 → 注入新任务写失败 → 检查事务回滚 → 恢复写入 → 重试 → 重开。
    /// 预期结果：失败时只有原2条消息；成功后增加1条用户意见和1项原Agent任务，原执行事件不变。
    func testRevisionKeepsOriginalArtifactAndAtomicallyCreatesOneFollowup() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let (store, operation, other) = try await fixture(path)
        let completed = try receipt(operation, sequence: 1, state: "done", text: "原回复", delivery: true)
        try await store.applyThreadTaskReceipt(completed, deviceID: "phone-a")
        let draft = WorkThreadDraft(text: "把按钮改为蓝色")
        try await store.saveThreadReviewDraft(draft, operationID: operation.id)
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(path.path, &db), SQLITE_OK, "打开隔离数据库注入任务写失败")
        defer { sqlite3_close(db) }
        XCTAssertEqual(sqlite3_exec(db, "CREATE TRIGGER fail_revision BEFORE INSERT ON thread_task_outbox BEGIN SELECT RAISE(ABORT,'revision failed'); END", nil, nil, nil), SQLITE_OK, "故障只作用于本次临时数据库")
        do { _ = try await store.requestThreadRevision(operationID: operation.id, draft: draft); XCTFail("写任务失败不能留下意见消息") }
        catch { XCTAssertTrue(error is WorkThreadError, "事务失败必须明确返回") }
        let failed = try await store.threadTask(operationID: operation.id)
        let before = try await store.messages(threadID: operation.task.threadID)
        XCTAssertEqual(failed.reviewDraft, draft, "失败时原意见仍在草稿")
        XCTAssertNil(failed.revisionOperationID, "失败不能标为已退回")
        XCTAssertEqual(before.map(\.author), ["user", "agent"], "失败不能留半条用户意见")
        XCTAssertEqual(sqlite3_exec(db, "DROP TRIGGER fail_revision", nil, nil, nil), SQLITE_OK, "恢复任务写入")
        let revision = try await store.requestThreadRevision(operationID: operation.id, draft: draft)
        let duplicate = try await store.requestThreadRevision(operationID: operation.id, draft: draft)
        XCTAssertEqual(duplicate.id, revision.id, "重复确认只能返回原修改任务")
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let original = try await restarted.threadTask(operationID: operation.id)
        let history = try await restarted.messages(threadID: operation.task.threadID)
        let elsewhere = try await restarted.threadTasks(threadID: other.id)
        XCTAssertEqual(original.receipt, completed, "原执行事件和成果不得被退回操作改写")
        XCTAssertEqual(original.revisionOperationID, revision.id, "原成果必须能定位后续任务")
        XCTAssertNil(original.reviewDraft, "仅成功发送的意见草稿应清除")
        XCTAssertEqual(history.map(\.text), ["@开发 修复首屏交接", "原回复", "把按钮改为蓝色"], "修改意见必须长期留在原线程")
        XCTAssertEqual(revision.task.revisionOf?.operationID, operation.id, "修改任务须引用原派工")
        XCTAssertEqual(revision.task.revisionOf?.taskRef, "runtime-a:7", "不能只用会复用的数字任务ID")
        XCTAssertEqual(revision.task.agentDefinitionID, 2, "修改必须交给原Agent")
        XCTAssertEqual(revision.machineID, operation.machineID, "修改不能换Mac")
        XCTAssertTrue(elsewhere.isEmpty, "其他线程不能收到修改任务")
        do { _ = try await restarted.requestThreadAcceptance(operationID: operation.id); XCTFail("退回后应验收下一份成果") }
        catch { XCTAssertTrue(error is WorkThreadError, "互斥决定必须明确拒绝") }
    }
    /// 验收决定单独持久保存，原执行事件与回复不变，错误回执不得显示已验收。
    /// 数据构造：1 个完成任务、1 份成果、1 次独立验收；非零时间戳沿用 fixture 保存时间。
    /// 执行过程：成果 → 请求验收 → 重开 → 错指纹 → 正确/重复/迟到回执 → 重开。
    /// 预期结果：确认前仍待同步；确认后不再轮询；原线程只有一条 Agent 回复且原成果不变。
    func testAcceptancePersistsWithoutRewritingExecutionOrDuplicatingReplies() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let (store, operation, _) = try await fixture(path)
        let completed = try receipt(operation, sequence: 1, state: "done", text: "原回复", delivery: true)
        try await store.applyThreadTaskReceipt(completed, deviceID: "phone-a")
        let pending = try await store.requestThreadAcceptance(operationID: operation.id)
        let again = try await store.requestThreadAcceptance(operationID: operation.id)
        XCTAssertEqual(again.acceptance, pending.acceptance, "重复点击只能保存同一验收操作")
        XCTAssertNotEqual(pending.acceptance?.decision.operationID, operation.id, "验收与原派工身份必须独立")
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let restored = try await restarted.threadTask(operationID: operation.id)
        XCTAssertEqual(restored.acceptance, pending.acceptance, "重启不能丢失决定或生成新ID")
        XCTAssertNil(restored.acceptance?.acceptance, "手机意愿不等于Mac已验收")
        let queue = try await restarted.pendingThreadTasks(includingExecution: true)
        XCTAssertEqual(queue.map(\.id), [operation.id], "验收与原任务共用有限同步队列")
        let state = try XCTUnwrap(restored.acceptance)
        let decision = try JSONSerialization.jsonObject(with: JSONEncoder().encode(state.decision))
        let delivery = try XCTUnwrap(completed.execution?.execution.delivery)
        let key = try JSONSerialization.jsonObject(with: JSONEncoder().encode(delivery.outcome.key))
        var accepted: [String: Any] = ["workspace_id": "project-a", "route_id": "accept-route", "key": key,
            "fingerprint": String(repeating: "f", count: 64), "device_id": "phone-a",
            "operation_id": state.decision.operationID, "accepted_at_unix": delivery.savedAtUnix]
        func result(_ acceptance: [String: Any]) throws -> ThreadAcceptanceState {
            try JSONDecoder().decode(ThreadAcceptanceState.self, from: JSONSerialization.data(withJSONObject: [
                "decision": decision, "acceptance": acceptance,
                "receipt": ["device_id": "phone-a", "operation_id": state.decision.operationID, "workspace_id": "project-a",
                    "route_id": "accept-route", "state": "succeeded", "result": ["node_id": 2, "run_id": "run-a",
                        "session_id": "session-a", "turn_id": "turn-a", "fingerprint": delivery.fingerprint,
                        "accepted_at_unix": delivery.savedAtUnix]]]))
        }
        do { try await restarted.applyThreadAcceptance(result(accepted), operationID: operation.id); XCTFail("错成果不能通过验收") }
        catch { XCTAssertTrue(error is MobileError, "错成果必须明确拒绝") }
        let unchanged = try await restarted.threadTask(operationID: operation.id)
        XCTAssertNil(unchanged.acceptance?.acceptance, "失败事务不得留下半份验收")
        accepted["fingerprint"] = delivery.fingerprint
        let confirmed = try result(accepted)
        try await restarted.applyThreadAcceptance(confirmed, operationID: operation.id)
        try await restarted.applyThreadAcceptance(confirmed, operationID: operation.id)
        try await restarted.applyThreadAcceptance(state, operationID: operation.id)
        let finalStore = CollaborationWorkspaceStore(databaseURL: path)
        let final = try await finalStore.threadTask(operationID: operation.id)
        let history = try await finalStore.messages(threadID: operation.task.threadID)
        let remaining = try await finalStore.pendingThreadTasks(includingExecution: true)
        XCTAssertEqual(final.acceptance, confirmed, "重复及迟到响应不能使已验收回退")
        XCTAssertEqual(final.receipt, completed, "验收不改写不可变执行事件")
        XCTAssertEqual(history.map(\.text), ["@开发 修复首屏交接", "原回复"], "不得增加伪造回复或重派指令")
        XCTAssertFalse(remaining.contains { $0.id == operation.id }, "验收落盘后不再重复占用投递预算")
    }
    /// 执行结果回到原线程；重启、重复查询、迟到接单均不能复制或删除 Agent 回复。
    /// 数据构造：同 Workspace 下 2 个线程，1 个任务 runtime-a:7；序号 1 运行、2 完成、3 成果晚到。
    /// 执行过程：保存并签收来源 → 应用运行/完成事件 → 重复完成 → 迟到接单 → 成果补齐 → 重开。
    /// 预期结果：原线程保留用户与 Agent 各 1 条；另一线程为空，Agent 回复不进入待发送队列。
    func testCompletedReplySurvivesRestartAndLateDeliveryWithoutDuplicates() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let (store, operation, other) = try await fixture(path)
        try await store.applyThreadTaskReceipt(receipt(operation, sequence: 1, state: "submitted"), deviceID: "phone-a")
        let running = try await store.messages(threadID: operation.task.threadID)
        XCTAssertEqual(running.map(\.author), ["user"], "运行中不能生成完成回复")
        let completed = try receipt(operation, sequence: 2, state: "done", text: "已修复首屏交接。")
        try await store.applyThreadTaskReceipt(completed, deviceID: "phone-a")
        try await store.applyThreadTaskReceipt(completed, deviceID: "phone-a")
        try await store.applyThreadTaskReceipt(receipt(operation, dispatch: "accepted"), deviceID: "phone-a")
        try await store.applyThreadTaskReceipt(receipt(operation, sequence: 3, state: "done", text: "已修复首屏交接。", delivery: true), deviceID: "phone-a")
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let messages = try await restarted.messages(threadID: operation.task.threadID)
        let elsewhere = try await restarted.messages(threadID: other.id)
        let pending = try await restarted.pendingThreadDeliveries()
        XCTAssertEqual(messages.map(\.author), ["user", "agent"], "原线程必须只追加一次真实 Agent 回复")
        XCTAssertEqual(messages.last?.text, "已修复首屏交接。", "Agent 回复应长期保留原文")
        XCTAssertEqual(messages.last?.state, .delivered, "收到的回复不应显示待发送")
        XCTAssertTrue(elsewhere.isEmpty, "同 Workspace 的其他线程不能混入回复")
        XCTAssertTrue(pending.isEmpty, "Agent 回复不能被重新作为用户指令投递")
    }

    /// 已派工任务继续查询执行结果，与新任务共用待同步队列；成果晚到不能遗漏。
    /// 数据构造：1 个已派工任务，事件序号 1 完成、2 成果补齐；另一个新任务尚未发送。
    /// 执行过程：应用派工回执 → 查询两种队列 → 完成但无成果 → 成果补齐 → 重开。
    /// 预期结果：已派工不重发；等待成果时仍同步；成果齐全后仅新任务留在同步队列。
    func testExecutionPollingSharesTaskQueueUntilLateDeliveryArrives() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let (store, operation, _) = try await fixture(path)
        try await store.applyThreadTaskReceipt(receipt(operation), deviceID: "phone-a")
        let fresh = try await store.enqueueThreadTask(sourceMessageID: operation.task.sourceMessageID, operationID: UUID().uuidString,
            target: try XCTUnwrap(operation.target), task: "下一项", contextVersion: 1, context: "")
        let dispatch = try await store.pendingThreadTasks()
        let syncing = try await store.pendingThreadTasks(includingExecution: true)
        XCTAssertEqual(dispatch.map(\.id), [fresh.id], "原任务已派工，不能再次派发")
        XCTAssertEqual(syncing.map(\.id), [operation.id, fresh.id], "执行查询与新任务必须共享轮转队列")
        try await store.applyThreadTaskReceipt(receipt(operation, sequence: 1, state: "done", text: "完成"), deviceID: "phone-a")
        let awaitingDelivery = try await store.pendingThreadTasks(includingExecution: true)
        XCTAssertEqual(awaitingDelivery.map(\.id), [operation.id, fresh.id], "终态回复先到不能停止成果同步")
        try await store.applyThreadTaskReceipt(receipt(operation, sequence: 2, state: "done", text: "完成", delivery: true), deviceID: "phone-a")
        let restarted = CollaborationWorkspaceStore(databaseURL: path)
        let remaining = try await restarted.pendingThreadTasks(includingExecution: true)
        XCTAssertEqual(remaining.map(\.id), [fresh.id], "原任务全部保存后不再占用每五秒的查询预算")
        XCTAssertFalse(remaining.contains { $0.id == operation.id }, "重启不能使终态任务重回投递队列")
    }

    /// 错任务/节点/成员/成果范围与冲突事件不能改写原结果；本机写失败不能留下半份回复。
    /// 数据构造：1 个任务 runtime-a:7；错误 runtime-b:7 复用数字 7，原节点 2 被伪造成 3。
    /// 执行过程：运行事件 → 逐项错身份 → 注入回复写失败 → 恢复写入完成 → 迟到/冲突/越限 → 重开。
    /// 预期结果：失败时仍为运行且无回复；完成后只有原文，旧序号/新冲突不可覆盖，来源可作为共同决定。
    func testInvalidEventsAndFailedWritesPreserveOriginalTaskAndReply() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let (store, operation, other) = try await fixture(path)
        let running = try receipt(operation, sequence: 1)
        try await store.applyThreadTaskReceipt(running, deviceID: "phone-a")
        await reject(store, try receipt(operation, sequence: 2) { $0["task_ref"] = "runtime-b:7" }, "完整引用不同必须拒绝")
        await reject(store, try receipt(operation, sequence: 2) { $0["node_id"] = 3 }, "另一节点不能覆盖")
        await reject(store, try receipt(operation, sequence: 2) { $0["agent_definition_id"] = 3 }, "另一成员不能覆盖")
        await reject(store, try receipt(operation, sequence: 2, state: "done", delivery: true) {
            var delivery = $0["delivery"] as! [String: Any]; delivery["workspace_id"] = "other-project"; $0["delivery"] = delivery
        }, "成果不能属于另一项目")
        await reject(store, try receipt(operation, sequence: 2, text: "提前完成"), "非终态不能夹带最终回复")
        // Reply preview ceiling = 16 * 1024 bytes; one additional ASCII byte must reject the entire event.
        await reject(store, try receipt(operation, sequence: 2, state: "done", text: String(repeating: "x", count: 16 * 1024 + 1)), "超长回复不能绕过有界协议")
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(path.path, &db), SQLITE_OK, "打开隔离库注入事务失败")
        defer { sqlite3_close(db) }
        XCTAssertEqual(sqlite3_exec(db, "CREATE TRIGGER fail_reply BEFORE INSERT ON work_messages BEGIN SELECT RAISE(ABORT,'reply write failed'); END", nil, nil, nil), SQLITE_OK, "只阻止新增回复")
        let completed = try receipt(operation, sequence: 2, state: "done", text: "原回复")
        do { try await store.applyThreadTaskReceipt(completed, deviceID: "phone-a"); XCTFail("回复无法落盘时不能提交完成状态") }
        catch { XCTAssertTrue(error is WorkThreadError, "本机写失败应明确返回") }
        let unchanged = try await store.threadTask(operationID: operation.id)
        let noReply = try await store.messages(threadID: operation.task.threadID)
        XCTAssertEqual(unchanged.receipt, running, "状态与回复必须一起回滚")
        XCTAssertEqual(unchanged.progress?.count, 1, "回复写失败不能留下完成进展")
        XCTAssertEqual(unchanged.progress?.last?.executionState, "submitted", "失败事务须保持此前执行中状态")
        XCTAssertFalse(unchanged.progress?.contains { $0.executionState == "done" } == true, "未提交的完成状态不能进入时间线")
        XCTAssertEqual(noReply.map(\.author), ["user"], "失败事务不能留下半份回复")
        XCTAssertEqual(sqlite3_exec(db, "DROP TRIGGER fail_reply", nil, nil, nil), SQLITE_OK, "恢复隔离库写入")
        try await store.applyThreadTaskReceipt(completed, deviceID: "phone-a")
        try await store.applyThreadTaskReceipt(running, deviceID: "phone-a")
        try await store.applyThreadTaskReceipt(receipt(operation), deviceID: "phone-a")
        await reject(store, try receipt(operation, sequence: 2, state: "done", text: "同序号伪造"), "同事件不可变")
        await reject(store, try receipt(operation, sequence: 3, state: "submitted"), "新序号也不能把终态退回运行")
        await reject(store, try receipt(operation, sequence: 3, state: "done", text: "更换最终回复"), "终态不能换正文")
        let saved = try await store.threadTask(operationID: operation.id)
        let history = try await store.messages(threadID: operation.task.threadID)
        let reply = try XCTUnwrap(history.last)
        XCTAssertEqual(saved.receipt, completed, "错误与迟到回执不能修改已保存结果")
        XCTAssertEqual(reply.text, "原回复", "最终回复应保留原文")
        let source = try await store.sourceMessage(id: reply.id, workspaceID: operation.task.collaborationWorkspaceID)
        XCTAssertEqual(source.taskOperationID, operation.id, "引用 Agent 回复必须保留原任务身份")
        _ = try await store.saveFact(threadID: other.id, kind: .decision, text: reply.text, sourceMessageID: reply.id)
        let facts = try await store.facts(threadID: other.id)
        XCTAssertEqual(facts.first?.sourceMessageID, reply.id, "共同决定可指回原线程 Agent 回复")
        do { _ = try await store.prepareThreadDelivery(messageID: reply.id); XCTFail("Agent 回复不可作为待发用户消息") }
        catch { XCTAssertTrue(error is WorkThreadError, "不允许为回复生成新投递操作") }
        XCTAssertEqual(sqlite3_exec(db, "DELETE FROM work_messages WHERE id='\(reply.id)'", nil, nil, nil), SQLITE_OK, "仅移除隔离库中已保存的回复，模拟局部损坏")
        do { _ = try await store.threadTask(operationID: operation.id); XCTFail("回复缺失必须报告存储损坏，不能默认为完整同步") }
        catch { XCTAssertTrue(error is WorkThreadError, "保存的任务与回复必须彼此核验") }
    }

    private func reject(_ store: CollaborationWorkspaceStore, _ event: ThreadTaskReceipt, _ reason: String) async {
        do { try await store.applyThreadTaskReceipt(event, deviceID: "phone-a"); XCTFail(reason) }
        catch { XCTAssertTrue(error is ThreadTaskError || error is MobileError, reason) }
    }

    /// 为真实原生 UI 验收准备隔离数据；该入口仅链接测试目标，不向 App 添加虚假完成路径。
    /// 数据构造：1 个完成任务、1 条真实存储格式回复、1 份冻结成果元数据；均明确为 fixture。
    /// 执行过程：显式提供全新测试库 → 使用公开 Store 写来源与结果 → 校验本机回复数量。
    /// 预期结果：UI 可验收已保存状态；未提供路径时跳过，不访问用户工作空间或真实 Agent。
    func testPrepareExecutionUIEvidence() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_EXECUTION_UI_DATABASE"] else { throw XCTSkip("仅供显式本机 UI fixture 准备") }
        XCTAssertFalse(FileManager.default.fileExists(atPath: path), "UI fixture 必须使用全新隔离库")
        guard !FileManager.default.fileExists(atPath: path) else { return }
        let (store, operation, _) = try await fixture(URL(fileURLWithPath: path))
        _ = try await store.saveFact(threadID: operation.task.threadID, kind: .goal, text: "从 Mac 切到 iPhone 时，让首屏只突出下一步。")
        // Actual fixture receipt order; no inferred steps. Each update is ten seconds after the prior.
        let observed = Date.now
        try await store.applyThreadTaskReceipt(receipt(operation, dispatch: "accepted"), deviceID: "phone-a", at: observed)
        try await store.applyThreadTaskReceipt(receipt(operation, sequence: 1), deviceID: "phone-a", at: observed.addingTimeInterval(10))
        try await store.applyThreadTaskReceipt(receipt(operation, sequence: 2, state: "done",
            text: "已核对首屏导航与信息层级，整理了下一步入口。请查看本轮任务结果。", delivery: true) {
                $0["result"] = ["text": "已核对首屏导航与信息层级，整理了下一步入口。请查看本轮任务结果。", "truncated": true]
            }, deviceID: "phone-a", at: observed.addingTimeInterval(20))
        let messages = try await store.messages(threadID: operation.task.threadID)
        XCTAssertEqual(messages.map(\.author), ["user", "agent"], "UI 应基于同一公开存储链路读取回复")
        let pending = try await store.pendingThreadTasks(includingExecution: true)
        XCTAssertTrue(pending.isEmpty, "完整 fixture 不应触发新的任务派发")
    }

    private let mac = "mac-" + String(repeating: "a", count: 64)

    private func fixture(_ path: URL) async throws -> (CollaborationWorkspaceStore, ThreadTaskOutboxOperation, WorkThread) {
        let store = CollaborationWorkspaceStore(databaseURL: path)
        var workspace = try await store.create(name: "结果回传")
        let target = WorkspaceAgentReference(machineID: mac, projectID: "project-a", definitionID: 2, name: "开发", profile: "Codex")
        workspace.projects = [.init(machineID: mac, projectID: "project-a", name: "iOS", machineName: "Mac")]
        workspace.agents = [target]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "交接")
        let other = try await store.createThread(workspaceID: workspace.id, title: "另一件事")
        let operation = try await store.appendTaskMessage(threadID: thread.id, messageID: UUID().uuidString,
            text: "@开发 修复首屏交接", operationID: UUID().uuidString, target: target, task: "修复首屏交接", contextVersion: 1, context: "")
        let outbox = try await store.prepareThreadDelivery(messageID: operation.task.sourceMessageID)
        let source = try XCTUnwrap(outbox.message)
        // 隔离 fixture 的第一条 Mac 签收 cursor=1，时间戳为固定非零测试时间。
        try await store.applyThreadReceipt(.init(sourceMacID: mac, collaborationWorkspaceID: workspace.id,
            threadID: thread.id, operationID: source.operationID, messageID: source.messageID, originDeviceID: source.originDeviceID,
            fingerprint: source.fingerprint(machineID: mac), receivedAtUnix: 1_790_000_000,
            eventID: mac + ":1", cursor: 1, state: "received"))
        _ = try await store.prepareThreadTask(operationID: operation.id, deviceID: "phone-a")
        return (store, operation, other)
    }

    private func receipt(_ operation: ThreadTaskOutboxOperation, dispatch: String = "succeeded", sequence: UInt64? = nil,
                         state: String = "submitted", text: String? = nil, delivery: Bool = false,
                         modify: (inout [String: Any]) -> Void = { _ in }) throws -> ThreadTaskReceipt {
        let task = operation.task
        let encoder = JSONEncoder(); encoder.outputFormatting = .sortedKeys
        let dispatchResult: Any = dispatch == "succeeded"
            ? ["status": "accepted", "node_id": 2, "task_id": 7, "task_ref": "runtime-a:7"] as [String: Any]
            : NSNull()
        var value: [String: Any] = [
            "binding": ["source_mac_id": mac, "collaboration_workspace_id": task.collaborationWorkspaceID,
                "thread_id": task.threadID, "source_operation_id": task.sourceOperationID, "source_message_id": task.sourceMessageID,
                "payload": String(decoding: try encoder.encode(task), as: UTF8.self)],
            "fingerprint": try task.fingerprint(machineID: mac),
            "receipt": ["device_id": "phone-a", "operation_id": operation.id, "workspace_id": task.projectID,
                "route_id": "route-a", "state": dispatch,
                "result": dispatchResult]
        ]
        if let sequence {
            var execution: [String: Any] = ["task_ref": "runtime-a:7", "node_id": 2, "agent_definition_id": 2,
                "state": state, "tracking": "managed"]
            if let text { execution["result"] = ["text": text, "truncated": false] }
            if delivery {
                execution["delivery"] = ["sequence": 1, "workspace_id": "project-a",
                    "outcome": ["key": ["node_id": 2, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a"], "state": "completed"],
                    "fingerprint": String(repeating: "b", count: 64), "saved_at_unix": 1_790_000_000, "status": "available", "file_count": 1]
            }
            modify(&execution)
            value["execution"] = ["sequence": sequence, "event_id": mac + ":execution:\(sequence)", "execution": execution]
        }
        return try JSONDecoder().decode(ThreadTaskReceipt.self, from: JSONSerialization.data(withJSONObject: value))
    }
}

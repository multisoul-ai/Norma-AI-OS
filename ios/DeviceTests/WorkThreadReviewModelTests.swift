import XCTest
@testable import Norma
@testable import NormaCore

@MainActor final class WorkThreadReviewModelTests: XCTestCase {
    /// 已保存文件的失败或停止任务仍可查看，但不能暗示成功或无解释地禁用验收。
    /// 数据构造：同一隔离任务的 completed/failed/interrupted 三种最终成果，文件数均为1。
    /// 执行过程：分别装入实际成果元数据 → 读取页面模型的状态与验收限制原因。
    /// 预期结果：完成可进入验收；失败/停止不显示待验收，必须说明不能通过并保留修改入口。
    @MainActor func testIncompleteDeliveryExplainsWhyItCannotBeAccepted() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let (store, operation) = try await fixture(directory)
        let completed = WorkThreadReviewModel(operation: try withOutcome(operation, state: .completed))
        XCTAssertEqual(completed.status, "待验收", "只有完成成果进入待验收状态")
        XCTAssertNil(completed.acceptanceRestriction, "完成且没有文件冲突时不额外限制验收")
        let failed = WorkThreadReviewModel(operation: try withOutcome(operation, state: .failed))
        XCTAssertEqual(failed.status, "执行失败", "失败文件不能标为成功待验收")
        XCTAssertNotNil(failed.acceptanceRestriction, "失败必须解释不能验收的原因")
        XCTAssertNotEqual(failed.resultSymbol, "checkmark.circle", "失败不能呈现绿色勾选语义")
        let stopped = WorkThreadReviewModel(operation: try withOutcome(operation, state: .interrupted))
        XCTAssertEqual(stopped.status, "已停止", "停止状态必须明确区别完成")
        XCTAssertNotNil(stopped.acceptanceRestriction, "停止成果必须解释验收限制")
        XCTAssertNotEqual(stopped.resultSymbol, "checkmark.circle", "停止不能呈现成功图标")
        let persisted = try await store.threadTask(operationID: operation.id)
        XCTAssertNil(persisted.acceptance, "查看状态不应创建验收决定")
    }

    /// 修改意见在连续编辑/后台退出时完成本地写入，重新加载不能覆盖刚输入的意见。
    /// 数据构造：1项隔离任务，2次连续文本；另一个线程聊天草稿不参与保存。
    /// 执行过程：加载→连续编辑→触发后台保存→重读→等待写入→重建模型。
    /// 预期结果：保留最后意见；没有发消息、创建修改任务或验收决定。
    @MainActor func testReviewDraftFinishesBeforeModelIsReleased() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let (store, operation) = try await fixture(directory)
        let model = WorkThreadReviewModel(operation: operation)
        await model.load(store)
        model.updateDraft("调整", store: store)
        model.updateDraft("调整首屏间距", store: store)
        model.finishDraftWrites()
        await model.load(store)
        await model.flushDraft()
        let restored = WorkThreadReviewModel(operation: operation)
        await restored.load(store)
        XCTAssertEqual(restored.draft?.text, "调整首屏间距", "最后编辑必须跨模型重建保存")
        XCTAssertNil(restored.operation.revisionOperationID, "保存意见草稿不得自动派工")
        XCTAssertNil(restored.operation.acceptance, "保存意见不得自动验收")
        let tasks = try await store.threadTasks(threadID: operation.task.threadID)
        XCTAssertEqual(tasks.count, 1, "草稿不能产生第二个任务")
    }

    /// 没有文字回复的成果跟随实际任务卡；具名回复存在时成果跟随该回复。
    /// 数据构造：1个用户指令、1项带冻结文件的完成任务；追加1条Norma分工、1条Agent回复。
    /// 执行过程：直接派工→加入Norma分工→补入具名回复，分别计算三种时间线归属。
    /// 预期结果：直接派工保留原位置；Norma分工后用户消息下无卡；具名回复下仅成果且无重复。
    func testTimelineKeepsSilentResultAfterNormaTask() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let (store, pending) = try await fixture(directory)
        var operation = try withOutcome(pending, state: .completed, text: "")
        let history = try await store.messages(threadID: operation.task.threadID)
        let user = try XCTUnwrap(history.first, "正式存储必须已有用户指令")
        let direct = user.timelineCards(tasks: [operation], history: history)
        XCTAssertEqual(direct.tasks.map(\.id), [operation.id], "直接派工的任务仍跟用户指令")
        XCTAssertEqual(direct.deliveries.map(\.id), [operation.id], "直接派工无文字结果仍有成果入口")
        var norma = WorkThreadMessage(id: UUID().uuidString, workspaceID: user.workspaceID, threadID: user.threadID,
            author: "norma", originDeviceID: user.originDeviceID, text: "交给开发检查首屏", createdAt: .now, state: .delivered)
        norma.replyToMessageID = user.id
        let plannedHistory = history + [norma]
        let sourceCards = user.timelineCards(tasks: [operation], history: plannedHistory)
        let plannedCards = norma.timelineCards(tasks: [operation], history: plannedHistory)
        XCTAssertTrue(sourceCards.tasks.isEmpty, "Norma派工后用户消息不能重复挂任务")
        XCTAssertTrue(sourceCards.deliveries.isEmpty, "不能在Norma分工之前展示成果")
        XCTAssertEqual(plannedCards.tasks.map(\.id), [operation.id], "任务紧跟Norma分工")
        XCTAssertEqual(plannedCards.deliveries.map(\.id), [operation.id], "无文字回复成果须在Norma任务之后")
        let agent = WorkThreadMessage(id: UUID().uuidString, workspaceID: user.workspaceID, threadID: user.threadID,
            author: "agent", originDeviceID: user.originDeviceID, text: "已完成", createdAt: .now, state: .delivered)
        operation = try withOutcome(operation, state: .completed, text: agent.text)
        operation.replyMessageID = agent.id
        let finalHistory = plannedHistory + [agent]
        let coordination = norma.timelineCards(tasks: [operation], history: finalHistory)
        let result = agent.timelineCards(tasks: [operation], history: finalHistory)
        XCTAssertEqual(coordination.tasks.map(\.id), [operation.id], "具名回复不能搬走Norma分工任务")
        XCTAssertTrue(coordination.deliveries.isEmpty, "具名回复已承载成果，Norma下不重复展示")
        XCTAssertTrue(result.tasks.isEmpty, "Agent回复下不重复展示派工任务")
        XCTAssertEqual(result.deliveries.map(\.id), [operation.id], "具名Agent回复后仅展示其成果")
    }

    private func fixture(_ directory: URL) async throws -> (CollaborationWorkspaceStore, ThreadTaskOutboxOperation) {
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspaces.sqlite"))
        let mac = "mac-" + String(repeating: "a", count: 64)
        let target = WorkspaceAgentReference(machineID: mac, projectID: "ios", definitionID: 2, name: "开发", profile: "Codex")
        var workspace = try await store.create(name: "验收")
        workspace.projects = [.init(machineID: mac, projectID: "ios", name: "iOS", machineName: "Mac")]
        workspace.agents = [target]; try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let task = try await store.appendTaskMessage(threadID: thread.id, messageID: UUID().uuidString, text: "@开发 首屏",
            operationID: UUID().uuidString, target: target, task: "首屏", contextVersion: 1, context: "")
        return (store, task)
    }

    private func withOutcome(_ operation: ThreadTaskOutboxOperation, state: MobileTurnOutcome.State, text: String = "回执") throws -> ThreadTaskOutboxOperation {
        var operation = operation
        let task = operation.task
        let delivery = try JSONDecoder().decode(TaskDeliverySummary.self, from: JSONSerialization.data(withJSONObject: [
            "sequence": 1, "workspace_id": "ios", "outcome": ["key": ["node_id": 2, "profile": "Codex", "run_id": "run-a",
                "session_id": "session-a", "turn_id": "turn-a"], "state": state.rawValue], "fingerprint": String(repeating: "a", count: 64),
            "saved_at_unix": 1_790_000_000, "status": "available", "file_count": 1]))
        operation.receipt = try ThreadTaskReceipt(binding: .init(sourceMacID: operation.machineID, collaborationWorkspaceID: task.collaborationWorkspaceID,
            threadID: task.threadID, sourceOperationID: task.sourceOperationID, sourceMessageID: task.sourceMessageID,
            payload: String(decoding: JSONEncoder().encode(task), as: UTF8.self)), fingerprint: task.fingerprint(machineID: operation.machineID),
            receipt: .init(deviceID: "phone-a", operationID: task.operationID, workspaceID: "ios", routeID: "route-a", state: "succeeded",
                result: .init(status: "accepted", message: nil, nodeID: 2, taskID: 7, taskRef: "runtime-a:7"), error: nil),
            execution: .init(sequence: 1, eventID: operation.machineID + ":execution:1",
                execution: .init(taskRef: "runtime-a:7", nodeID: 2, agentDefinitionID: 2, state: state == .completed ? "done" : state.rawValue,
                    tracking: "managed", error: nil, result: .init(text: text, truncated: false), delivery: delivery)))
        return operation
    }
}

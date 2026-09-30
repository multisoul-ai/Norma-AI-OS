import Foundation
import XCTest
@testable import NormaCore

@MainActor final class RelayNetworkTests: XCTestCase {
    /// Swift 经正式 Node 中继与真实 Rust MobileServer 建立加密会话并读取授权项目。
    /// 数据构造：1 台已配对 Mac、1 个项目；云凭据来自真实邀请，Mac token 仅用于隔离测试。
    /// 执行过程：认证/读取 → 丢回执后重连核对 → 按批恢复其他线程 → 错误配对/取消/超时。
    /// 批次数据：1 条无项目消息 + 3 条有效消息；每批最多 2 条，故首批留下第 3 条待发。
    /// 预期结果：坏目标不阻塞其他线程；原文/回执不重复；无配置与超出批次的消息不能误标送达。
    /// 派工恢复：3 个 Swift 任务，首个故意不落手机回执；重建 Store 后只能查回原操作。
    ///   第三个需越过 2 个尚在排队的任务，批次=1，故第 3 轮才发送；Mac 检查仅新增这 3 次入口。
    /// 验收：读取 Node 已提交的原决定，再以新 route 重试；验收必须独立于原执行事件且不再入前台。
    func testNativeClientThroughRelayToMac() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_NATIVE_RELAY_FIXTURE"] else {
            throw XCTSkip("由 mobile_collaboration_relay e2e 启动正式本机中继与 Mac 服务后运行")
        }
        struct Fixture: Decodable {
            let port: UInt16
            let machineID: String
            let projectID: String
            let task: ThreadTaskOperation
            let revisionTask: ThreadTaskOperation
            let acceptanceDecision: ThreadAcceptanceDecision
            let cloud: CloudDeviceCredential
            let mac: DeviceCredential
            let silentPort: UInt16
            let redirectPort: UInt16
        }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let client = try CollaborationRelayClient(localPort: fixture.port)
        let connection = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        let capabilities = await connection.capabilities
        XCTAssertFalse(capabilities.contains("collaboration_v1"), "局部派工接口不能宣称完整执行事件回传能力")
        XCTAssertTrue(capabilities.contains("thread_delivery_v1"), "本次 Mac 必须提供实际持久接收能力")
        XCTAssertTrue(capabilities.contains("thread_tasks_v1"), "本次 Mac 必须提供已绑定来源的派工入口")
        let taskReceipt = try await connection.threadTaskReceipt(for: fixture.task)
        XCTAssertEqual(taskReceipt?.receipt.result?.taskID, 7, "Swift 必须解析原 Node 客户端收到的同一派工身份")
        let taskReplay = try await connection.submitThreadTask(fixture.task, routeID: "swift-refreshed-route", grantRevision: 1)
        XCTAssertEqual(taskReplay.receipt, taskReceipt?.receipt, "Swift 重连后更新 route 只能恢复原派工身份，不能重复派工")
        XCTAssertEqual(taskReplay.fingerprint, taskReceipt?.fingerprint, "可选执行事件不同不能改变原请求指纹")
        XCTAssertEqual(taskReceipt?.execution?.execution.taskRef, "test-runtime:7", "必须取完整任务引用，不得选 other-runtime:7")
        XCTAssertEqual(taskReceipt?.execution?.execution.nodeID, 2, "必须读取原节点，不得选后创建的节点 3")
        XCTAssertEqual(taskReceipt?.execution?.execution.result?.text, "原任务：首屏导航已修复", "Swift 必须解码原持久回复")
        XCTAssertEqual(taskReceipt?.execution?.execution.delivery?.fileCount, 1, "原回合冻结成果应保留准确文件数")
        let terminal = try await connection.threadTaskTerminal(task: fixture.task, receipt: XCTUnwrap(taskReceipt))
        XCTAssertEqual(terminal.key.turnID, "original-turn", "终端必须来自原命令的Provider回合")
        XCTAssertEqual(terminal.page.rows.last, "fixture-terminal-199", "经正式中继读取完整最新页")
        XCTAssertFalse(terminal.page.inputReady, "读取并不授予独占输入权")
        let history = try await connection.threadTaskTerminal(task: fixture.task, receipt: XCTUnwrap(taskReceipt), before: terminal)
        XCTAssertEqual(history.page.rows.first, "fixture-terminal-0", "历史分页不能丢失首行")
        XCTAssertEqual(history.key, terminal.key, "历史不能切换到另一回合")
        let frozenReview = try await connection.threadTaskReview(task: fixture.task, receipt: XCTUnwrap(taskReceipt))
        XCTAssertEqual(frozenReview.delivery?.fingerprint, taskReceipt?.execution?.execution.delivery?.fingerprint, "打开的成果必须属于原任务指纹")
        XCTAssertEqual(frozenReview.review.files.first?.after?.text, String(repeating: "结果\n", count: 20_000), "正式中继多片传输后必须完整恢复原文件")
        XCTAssertFalse(frozenReview.review.files.contains { $0.path != "result.txt" }, "不能混入同项目其他成果文件")
        var acceptanceOriginal = ThreadTaskOutboxOperation(machineID: fixture.machineID, task: fixture.task)
        acceptanceOriginal.deviceID = fixture.mac.deviceID; acceptanceOriginal.receipt = taskReceipt
        acceptanceOriginal.acceptance = ThreadAcceptanceState(decision: fixture.acceptanceDecision)
        let accepted = try await connection.threadTaskAcceptance(for: acceptanceOriginal)
        let acceptanceReplay = try await connection.threadTaskAcceptance(for: acceptanceOriginal, routeID: "swift-reconnected", grantRevision: 1)
        XCTAssertEqual(acceptanceReplay, accepted, "Swift恢复必须读取同一验收，不执行第二次")
        XCTAssertEqual(accepted.receipt?.state, "succeeded", "必须取回Mac持久成功回执")
        XCTAssertEqual(accepted.acceptance?.fingerprint, frozenReview.delivery?.fingerprint, "验收只能绑定已显示的冻结成果")
        XCTAssertNotEqual(accepted.acceptance?.operationID, fixture.task.operationID, "验收不能覆盖派工操作")
        let revision = try await connection.threadTaskReceipt(for: fixture.revisionTask)
        XCTAssertEqual(revision?.execution?.execution.taskRef, "revision-runtime:8", "Swift必须核验带原成果引用的新任务指纹")
        XCTAssertEqual(revision?.execution?.execution.delivery?.outcome.key.turnID, "revision-turn", "修改后的成果不能误作原回合")
        XCTAssertEqual(revision?.binding.threadID, taskReceipt?.binding.threadID, "修改成果继续返回原线程")
        XCTAssertEqual(taskReplay.receipt.result?.status, "accepted", "派工持久成功不等于 Agent 完成任务")
        let first = try await connection.snapshot()
        let second = try await connection.snapshot()
        XCTAssertEqual(first.projects.map(\.workspaceID), [fixture.projectID], "原生客户端必须读到真实 Mac 授权项目")
        XCTAssertEqual(second.projects.map(\.workspaceID), [fixture.projectID], "后续回合必须继续使用正确会话序号")
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: database)
        var workspace = try await store.create(name: "真实网络测试")
        workspace.projects = [.init(machineID: fixture.machineID, projectID: fixture.projectID, name: "iOS", machineName: "Mac")]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "独立线程")
        let original = try await store.appendMessage(threadID: thread.id, id: UUID().uuidString, text: "中文原文\n只同步讨论，不执行。")
        let operation = try await store.prepareThreadDelivery(messageID: original.id)
        let payload = try XCTUnwrap(operation.message)
        let received = try await connection.receiveThreadMessage(payload)
        XCTAssertEqual(received.fingerprint, try payload.fingerprint(machineID: fixture.machineID), "Swift 与 Rust 必须按相同字节规则确认原操作")
        // Model a lost acknowledgement: do not apply the received value before closing the phone session.
        await connection.close()
        do { _ = try await connection.snapshot(); XCTFail("主动关闭后不应隐式重连或重放请求") }
        catch { XCTAssertTrue(error is MobileError, "关闭后应返回明确连接错误") }
        let recovered = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        let restarted = CollaborationWorkspaceStore(databaseURL: database)
        let pending = try await restarted.messages(threadID: thread.id)
        XCTAssertEqual(pending.first?.state, .pending, "没有落入手机数据库的回执不能算已送达")
        let receipt = try await restarted.deliverThreadMessage(messageID: original.id, using: recovered)
        XCTAssertEqual(receipt, received, "重连先核对原操作，必须恢复原事件而非新增接收")
        let events = try await recovered.threadEvents(workspaceID: workspace.id, threadID: thread.id, after: 0)
        XCTAssertEqual(events, [received], "丢回执恢复不能产生第二个事件")
        let delivered = try await restarted.messages(threadID: thread.id)
        XCTAssertEqual(delivered.map(\.state), [.delivered], "只有真实 Mac 回执落盘后才能显示已送达")

        let target = WorkspaceAgentReference(machineID: fixture.machineID, projectID: fixture.projectID,
            definitionID: 2, name: "开发", profile: "Codex")
        workspace.agents = [target]
        try await restarted.save(workspace)
        let lost = try await restarted.enqueueThreadTask(sourceMessageID: original.id, operationID: UUID().uuidString,
            target: target, task: "Swift 丢回执恢复", contextVersion: 1, context: "本线程的目标")
        _ = try await restarted.prepareThreadTask(operationID: lost.id, deviceID: fixture.mac.deviceID)
        let beforeLoss = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        let lostReceipt = try await beforeLoss.submitThreadTask(lost.task, routeID: "swift-before-disconnect", grantRevision: 1)
        await beforeLoss.close() // Intentionally do not apply the real Mac receipt on the phone.
        let afterLoss = CollaborationWorkspaceStore(databaseURL: database)
        let frozen = try await afterLoss.threadTask(operationID: lost.id)
        XCTAssertNil(frozen.receipt, "丢失响应不能伪装本机已记录接单")
        workspace.agents = []
        try await afterLoss.save(workspace)
        let taskConnection = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        let restoredReceipt = try await afterLoss.deliverThreadTask(operationID: lost.id, using: taskConnection)
        XCTAssertEqual(restoredReceipt, lostReceipt, "移除成员后仍可恢复已发生的原接单，不能重派")
        workspace.agents = [target]
        try await afterLoss.save(workspace)
        let fresh = try await afterLoss.enqueueThreadTask(sourceMessageID: original.id, operationID: UUID().uuidString,
            target: target, task: "Swift 持久待发首次派工", contextVersion: 2, context: "仅当前线程")
        let freshReceipt = try await afterLoss.deliverThreadTask(operationID: fresh.id, using: taskConnection)
        XCTAssertEqual(freshReceipt.receipt.state, "accepted", "Mac 预留任务只能显示接单")
        XCTAssertEqual(freshReceipt.receipt.routeID, "relay-live-route", "首次派工必须读取当前项目路由")
        let taskRestart = CollaborationWorkspaceStore(databaseURL: database)
        let repeated = try await taskRestart.deliverThreadTask(operationID: fresh.id, using: taskConnection)
        XCTAssertEqual(repeated, freshReceipt, "重启轮询 accepted 只能查询原操作")
        let tasks = try await taskRestart.threadTasks(threadID: thread.id)
        XCTAssertEqual(tasks.map(\.id), [lost.id, fresh.id], "只能存在两次明确派工，不得产生恢复副本")
        XCTAssertNil(tasks.last?.receipt?.receipt.result, "排队不能伪造 Agent 产物")
        let later = try await taskRestart.enqueueThreadTask(sourceMessageID: original.id, operationID: UUID().uuidString,
            target: target, task: "Swift 批次轮转派工", contextVersion: 2, context: "同一冻结上下文")
        _ = try await taskRestart.deliverPendingThreadTasks(using: taskConnection)
        _ = try await taskRestart.deliverPendingThreadTasks(using: taskConnection)
        let notYet = try await taskRestart.threadTask(operationID: later.id)
        XCTAssertNil(notYet.receipt, "每批仅处理一个派工，前两批只能查询原排队任务")
        let nextBatch = try await taskRestart.deliverPendingThreadTasks(using: taskConnection)
        let sent = try await taskRestart.threadTask(operationID: later.id)
        XCTAssertEqual(nextBatch?.operationID, later.id, "原任务仍排队不能永久阻塞后续明确指令")
        XCTAssertNil(nextBatch?.error, "轮转到有效任务应正常送达")
        XCTAssertEqual(sent.receipt?.receipt.state, "accepted", "轮转到第三项应真实取得 Mac 持久回执")

        // Explicitly simulate a previously received command missing from the Mac's restored ledger.
        let missing = try await taskRestart.enqueueThreadTask(sourceMessageID: original.id, operationID: UUID().uuidString,
            target: target, task: "不可因服务端记录缺失重做", contextVersion: 1, context: "")
        let missingTask = missing.task
        let remembered = try ThreadTaskReceipt(binding: .init(sourceMacID: fixture.machineID,
            collaborationWorkspaceID: workspace.id, threadID: thread.id, sourceOperationID: missingTask.sourceOperationID,
            sourceMessageID: original.id, payload: String(decoding: JSONEncoder().encode(missingTask), as: UTF8.self)),
            fingerprint: missingTask.fingerprint(machineID: fixture.machineID),
            receipt: .init(deviceID: fixture.mac.deviceID, operationID: missing.id, workspaceID: fixture.projectID,
                routeID: "remembered-route", state: "accepted", result: nil, error: nil))
        try await taskRestart.applyThreadTaskReceipt(remembered, deviceID: fixture.mac.deviceID)
        do { _ = try await taskRestart.deliverThreadTask(operationID: missing.id, using: taskConnection); XCTFail("接单记录丢失不能自动重派") }
        catch ThreadTaskError.receiptMissing { }
        let preserved = try await taskRestart.threadTask(operationID: missing.id)
        XCTAssertEqual(preserved.receipt, remembered, "远端查询为空不能清除本机已接单证据")
        let dispatchedMissing = ThreadTaskReceipt(binding: remembered.binding, fingerprint: remembered.fingerprint,
            receipt: .init(deviceID: fixture.mac.deviceID, operationID: missing.id, workspaceID: fixture.projectID,
                routeID: "remembered-route", state: "succeeded", result: .init(status: "accepted", message: nil, nodeID: 2,
                    taskID: 9, taskRef: "missing-runtime:9"), error: nil))
        try await taskRestart.applyThreadTaskReceipt(dispatchedMissing, deviceID: fixture.mac.deviceID)
        do { _ = try await taskRestart.deliverThreadTask(operationID: missing.id, using: taskConnection); XCTFail("已派工的结果查询为空不能重新执行") }
        catch ThreadTaskError.receiptMissing { }
        let knownDispatch = try await taskRestart.threadTask(operationID: missing.id)
        XCTAssertEqual(knownDispatch.receipt, dispatchedMissing, "结果查询缺失仍须保留已派工身份")
        await taskConnection.close()

        let unconfigured = try await restarted.create(name: "未关联项目")
        let blocked = try await restarted.createThread(workspaceID: unconfigured.id, title: "未配置")
        let blockedMessage = try await restarted.appendMessage(threadID: blocked.id, id: UUID().uuidString, text: "等待关联")
        let another = try await restarted.createThread(workspaceID: workspace.id, title: "后台线程")
        let one = try await restarted.appendMessage(threadID: another.id, id: UUID().uuidString, text: "第一条")
        let two = try await restarted.appendMessage(threadID: another.id, id: UUID().uuidString, text: "第二条")
        let three = try await restarted.appendMessage(threadID: another.id, id: UUID().uuidString, text: "第三条")
        let failures = try await restarted.deliverPendingThreadMessages(using: recovered)
        XCTAssertNotNil(failures[blockedMessage.id], "未关联消息必须有独立错误，不能静默消失")
        XCTAssertNil(failures[one.id], "另一工作空间的配置错误不能阻塞有效消息")
        let batch = try await restarted.messages(threadID: another.id)
        XCTAssertEqual(batch.first { $0.id == one.id }?.state, .delivered, "第一条应真实送达")
        XCTAssertEqual(batch.first { $0.id == two.id }?.state, .delivered, "第二条应真实送达")
        XCTAssertEqual(batch.first { $0.id == three.id }?.state, .pending, "超出两条批次的消息仍待发送")
        _ = try await restarted.deliverPendingThreadMessages(using: recovered)
        let finalEvents = try await recovered.threadEvents(workspaceID: workspace.id, threadID: another.id, after: 0)
        XCTAssertEqual(finalEvents.map(\.messageID), [one.id, two.id, three.id], "下一批只能补第三条，不能重复先前消息")
        let blockedHistory = try await restarted.messages(threadID: blocked.id)
        XCTAssertEqual(blockedHistory.map(\.state), [.pending], "未配置线程不能继承其他线程的送达状态")
        await recovered.close()

        // Two denied messages fill one batch; a third message on an authorized project must get the next turn.
        var deniedWorkspace = try await restarted.create(name: "未授权项目")
        deniedWorkspace.projects = [.init(machineID: fixture.machineID, projectID: "not-granted", name: "受限", machineName: "Mac")]
        try await restarted.save(deniedWorkspace)
        let deniedThread = try await restarted.createThread(workspaceID: deniedWorkspace.id, title: "无权限")
        _ = try await restarted.appendMessage(threadID: deniedThread.id, id: UUID().uuidString, text: "被拒绝一")
        _ = try await restarted.appendMessage(threadID: deniedThread.id, id: UUID().uuidString, text: "被拒绝二")
        let healthy = try await restarted.appendMessage(threadID: another.id, id: UUID().uuidString, text: "仍有权限")
        let deniedConnection = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        _ = try await restarted.deliverPendingThreadMessages(using: deniedConnection)
        await deniedConnection.close()
        let nextConnection = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        _ = try await restarted.deliverPendingThreadMessages(using: nextConnection)
        await nextConnection.close()
        let healthyHistory = try await restarted.messages(threadID: another.id)
        let deniedHistory = try await restarted.messages(threadID: deniedThread.id)
        XCTAssertEqual(healthyHistory.first { $0.id == healthy.id }?.state, .delivered, "失败操作不能永久占据每个批次，阻塞其他授权线程")
        XCTAssertEqual(deniedHistory.map(\.state), [.pending, .pending], "轮转重试不能给未授权项目伪造回执")
        do {
            _ = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud,
                credential: DeviceCredential(deviceID: fixture.mac.deviceID, token: String(repeating: "b", count: 64)))
            XCTFail("云准入不能绕过 Mac 配对凭据")
        } catch { XCTAssertTrue(error is MobileError, "配对认证失败应结束整个通道") }
        // The inherited transport has a 5s request ceiling; cancellation must not wait for it.
        let silent = try CollaborationRelayClient(localPort: fixture.silentPort)
        let cancelled = Task {
            try await silent.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        }
        // 100ms lets the local socket begin IO; correctness does not require it to be connected yet.
        try await Task.sleep(for: .milliseconds(100))
        let clock = ContinuousClock(), started = ContinuousClock.now
        cancelled.cancel()
        do { _ = try await cancelled.value; XCTFail("取消不能返回可用会话") }
        catch is CancellationError { }
        XCTAssertLessThan(clock.now - started, .seconds(2), "取消须释放正在等待的网络 IO，不能等满 5s")
        do {
            _ = try await silent.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
            XCTFail("不回应的中继不能无限占用连接")
        } catch MobileError.timedOut { }
        let redirect = try CollaborationRelayClient(localPort: fixture.redirectPort)
        do {
            _ = try await redirect.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
            XCTFail("含云凭据的 WebSocket 请求不能跟随重定向")
        } catch { XCTAssertTrue(error is MobileError, "重定向必须作为连接失败处理") }
    }
}

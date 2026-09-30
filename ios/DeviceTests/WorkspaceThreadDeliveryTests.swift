import XCTest
@testable import Norma
@testable import NormaCore

final class WorkspaceThreadDeliveryTests: XCTestCase {
    /// App 前台恢复必须扫描独立任务待发记录，不能只看已送达的来源消息。
    /// 数据构造：1 个已配置工作空间、1 条来源及 1 个任务；本机 fixture 签收来源，云凭据缺失。
    /// 执行过程：保存呼叫 → 来源回执 → 终端占用/替换/旧页退出 → 前台恢复 → 派工回执再恢复 → 终端连接取消。
    /// 预期结果：失败保留原操作与已送达消息；执行结果与独立追问查询不能漏掉；连接取消清理旧成功提示、输入权与预算，保留草稿。
    @MainActor func testForegroundRecoveryIncludesTaskAfterSourceWasDelivered() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: path)
        let mac = "mac-" + String(repeating: "a", count: 64)
        let agent = WorkspaceAgentReference(machineID: mac, projectID: "ios", definitionID: 2, name: "开发", profile: "Codex")
        var workspace = try await store.create(name: "任务")
        workspace.projects = [.init(machineID: mac, projectID: "ios", name: "iOS", machineName: "Mac")]
        workspace.agents = [agent]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "派工")
        let messageID = UUID().uuidString
        let queued = try await store.appendTaskMessage(threadID: thread.id, messageID: messageID, text: "@开发 修导航",
            operationID: UUID().uuidString, target: agent, task: "修导航", contextVersion: 1, context: "")
        let source = try await store.prepareThreadDelivery(messageID: messageID)
        let payload = try XCTUnwrap(source.message)
        // 第一条测试接收事件 cursor=1，独立存储 fixture，不冒充真实 Mac 网络。
        try await store.applyThreadReceipt(.init(sourceMacID: mac, collaborationWorkspaceID: workspace.id,
            threadID: thread.id, operationID: source.id, messageID: messageID, originDeviceID: payload.originDeviceID,
            fingerprint: payload.fingerprint(machineID: mac), receivedAtUnix: 1_790_000_000, eventID: mac + ":1", cursor: 1, state: "received"))
        let reopened = CollaborationWorkspaceStore(databaseURL: path)
        let model = WorkspaceLibraryModel(store: reopened)
        let firstTerminal = model.beginTerminalSession(machineID: mac)
        await model.deliverPending(machines: [], cloud: nil, vault: CredentialVault())
        XCTAssertNil(model.taskDeliveryErrors[queued.id], "终端活跃时同Mac后台投递不得争抢请求额度")
        let nextTerminal = model.beginTerminalSession(machineID: mac)
        model.endTerminalSession(machineID: mac, id: firstTerminal)
        await model.deliverPending(machines: [], cloud: nil, vault: CredentialVault())
        XCTAssertNil(model.taskDeliveryErrors[queued.id], "旧页面退出不得释放新终端占用")
        model.endTerminalSession(machineID: mac, id: nextTerminal)
        await model.deliverPending(machines: [], cloud: nil, vault: CredentialVault())
        XCTAssertEqual(model.taskDeliveryErrors[queued.id], CloudTextError.unauthorized.localizedDescription, "已送达消息之后的待发任务必须仍被恢复扫描")
        XCTAssertTrue(model.deliveryErrors.isEmpty, "任务错误不能回写成来源消息投递失败")
        XCTAssertFalse(model.deliveryInProgress, "失败后释放前台投递循环")
        let saved = try await reopened.pendingThreadTasks()
        let messages = try await reopened.messages(threadID: thread.id)
        XCTAssertEqual(saved, [queued], "失败不能修改任务身份、上下文或伪造回执")
        XCTAssertEqual(messages.first?.state, .delivered, "来源消息的持久回执仍须保留")
        let task = queued.task
        let dispatched = try ThreadTaskReceipt(binding: .init(sourceMacID: mac, collaborationWorkspaceID: workspace.id,
            threadID: thread.id, sourceOperationID: source.id, sourceMessageID: messageID,
            payload: String(decoding: JSONEncoder().encode(task), as: UTF8.self)), fingerprint: task.fingerprint(machineID: mac),
            receipt: .init(deviceID: "phone-a", operationID: task.operationID, workspaceID: "ios", routeID: "route-a", state: "succeeded",
                result: .init(status: "accepted", message: nil, nodeID: 2, taskID: 7, taskRef: "runtime-a:7"), error: nil))
        try await reopened.applyThreadTaskReceipt(dispatched, deviceID: "phone-a")
        let recoveringResult = WorkspaceLibraryModel(store: reopened)
        await recoveringResult.deliverPending(machines: [], cloud: nil, vault: CredentialVault())
        XCTAssertEqual(recoveringResult.taskDeliveryErrors[queued.id], CloudTextError.unauthorized.localizedDescription, "App 必须继续恢复已派工任务的结果查询")
        let toDispatch = try await reopened.pendingThreadTasks()
        XCTAssertTrue(toDispatch.isEmpty, "执行同步失败不能使已派工任务重回待派发队列")
        let known = try await reopened.threadTask(operationID: queued.id)
        XCTAssertEqual(known.receipt, dispatched, "缺少连接授权不能丢失原派工身份")

        let followup = try await reopened.appendThreadFollowup(parentOperationID: queued.id, messageID: UUID().uuidString,
            text: "解释导航修改", operationID: UUID().uuidString)
        let followupSource = try await reopened.prepareThreadDelivery(messageID: followup.followup.sourceMessageID)
        let followupMessage = try XCTUnwrap(followupSource.message)
        // 第二条来源事件cursor=2，只验证前台恢复扫描，不冒充真实网络。
        try await reopened.applyThreadReceipt(.init(sourceMacID: mac, collaborationWorkspaceID: workspace.id,
            threadID: thread.id, operationID: followupSource.id, messageID: followupMessage.messageID,
            originDeviceID: followupMessage.originDeviceID, fingerprint: followupMessage.fingerprint(machineID: mac),
            receivedAtUnix: 1_790_000_000, eventID: mac + ":2", cursor: 2, state: "received"))
        let followupRecovery = WorkspaceLibraryModel(store: reopened)
        await followupRecovery.deliverPending(machines: [], cloud: nil, vault: CredentialVault())
        XCTAssertEqual(followupRecovery.taskDeliveryErrors[followup.id], CloudTextError.unauthorized.localizedDescription,
            "追问的来源已送达后，App仍须扫描独立追问操作并显示错误")
        XCTAssertNil(followupRecovery.deliveryErrors[followupMessage.messageID], "追问错误不得篡改来源送达")
        let stillPending = try await reopened.pendingThreadFollowups()
        XCTAssertEqual(stillPending.map(\.id), [followup.id], "云授权缺失必须保留原追问操作")

        // 已开始的投递占用同Mac；取消等待模拟页面任务退出，不连接模型或Mac网络。
        model.deliveryMachineID = mac
        let terminal = WorkThreadTerminalModel(operation: known)
        let cloud = CloudChatModel(store: CloudChatStore(databaseURL: directory.appendingPathComponent("cloud.sqlite")),
            vault: CloudCredentialVault(service: "norma-terminal-cancel-\(UUID().uuidString)"), client: CloudTextClient())
        let run = Task { await terminal.run(library: model, computers: ComputerModel(), cloud: cloud) }
        // 100 × 10ms = 1秒，仅等待本机MainActor建立占用，不依赖服务响应。
        for _ in 0..<100 where model.terminalSessions[mac] == nil { try await Task.sleep(for: .milliseconds(10)) }
        XCTAssertNotNil(model.terminalSessions[mac], "终端生命周期必须已经开始")
        terminal.notice = "已接管，Agent 自动输入已暂停。"
        terminal.draft = WorkThreadDraft(text: "尚未发送")
        run.cancel(); await run.value
        XCTAssertNil(terminal.notice, "连接生命周期结束不能残留上次接管成功提示")
        XCTAssertFalse(terminal.connected, "退出后的终端不得标为已连接")
        XCTAssertFalse(terminal.controlled, "退出后的终端不得保留输入权")
        XCTAssertEqual(terminal.draft?.text, "尚未发送", "撤销连接不能丢失本地输入草稿")
        XCTAssertNil(model.terminalSessions[mac], "结束连接必须归还同Mac投递预算")
    }

    /// 云初始化暂时失败时必须向线程暴露原因，下次前台检查应重新加载并恢复处理。
    /// 数据构造：1 条本机待发消息；用普通文件占据云数据库父目录，强制首次初始化失败。
    /// 执行过程：前台检查失败 → 移走隔离阻断文件 → 再次前台检查 → 查看消息与错误。
    /// 预期结果：首次错误可见；恢复后进入目标校验且原消息仍待发；未调用模型或外部网络。
    @MainActor func testCloudInitializationFailureIsVisibleAndRetried() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let blocked = directory.appendingPathComponent("cloud")
        try Data("temporary blocker".utf8).write(to: blocked)
        let cloud = CloudChatModel(store: CloudChatStore(databaseURL: blocked.appendingPathComponent("chat.sqlite")),
            vault: CloudCredentialVault(service: "norma-delivery-recovery-\(UUID().uuidString)"), client: CloudTextClient())
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspace.sqlite"))
        let workspace = try await store.create(name: "测试")
        let thread = try await store.createThread(workspaceID: workspace.id, title: "讨论")
        let message = try await store.appendMessage(threadID: thread.id, id: UUID().uuidString, text: "保留原文")
        let model = WorkspaceLibraryModel(store: store)
        await model.refreshDelivery(machines: [], cloud: cloud, vault: CredentialVault())
        XCTAssertNotNil(model.deliveryError, "初始化失败不能使线程静默停止投递")
        XCTAssertFalse(cloud.loaded, "失败不能伪装成已加载")
        try FileManager.default.removeItem(at: blocked)
        await model.refreshDelivery(machines: [], cloud: cloud, vault: CredentialVault())
        XCTAssertTrue(cloud.loaded, "下次前台检查应重试并恢复云初始化")
        XCTAssertNil(model.deliveryError, "恢复后应清除过时的全局错误")
        XCTAssertEqual(model.deliveryErrors[message.id], ThreadDeliveryError.unconfigured.localizedDescription, "恢复后应继续处理独立的消息目标错误")
        let messages = try await store.messages(threadID: thread.id)
        XCTAssertEqual(messages.map(\.state), [.pending], "初始化恢复不能伪造 Mac 回执")
    }

    /// 前台恢复同步时，未配置目标与缺少云授权分别报错，原文和待发身份不丢失。
    /// 数据构造：2 个 Workspace/线程；A 无项目，B 有 1 台 Mac/项目但无云凭据；各 1 条消息。
    /// 执行过程：落盘 → B终端占用时仍处理A → 释放B → App Model恢复投递 → 重建再恢复。
    /// 预期结果：两个失败按消息隔离，未连接网络，没有任一消息假报已送达，操作 ID 不变。
    @MainActor func testForegroundRecoveryKeepsIndependentPendingMessages() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspaces.sqlite"))
        let a = try await store.create(name: "无项目")
        var b = try await store.create(name: "已配置")
        b.projects = [.init(machineID: "mac-" + String(repeating: "a", count: 64), projectID: "ios", name: "iOS", machineName: "Mac")]
        try await store.save(b)
        let first = try await store.createThread(workspaceID: a.id, title: "A")
        let second = try await store.createThread(workspaceID: b.id, title: "B")
        let one = try await store.appendMessage(threadID: first.id, id: UUID().uuidString, text: "只讨论")
        let two = try await store.appendMessage(threadID: second.id, id: UUID().uuidString, text: "继续讨论")
        let original = try await store.pendingThreadDeliveries()
        let model = WorkspaceLibraryModel(store: store)
        let vault = CredentialVault()
        let terminal = model.beginTerminalSession(machineID: b.projects[0].machineID)
        await model.deliverPending(machines: [], cloud: nil, vault: vault)
        XCTAssertEqual(model.deliveryErrors[one.id], ThreadDeliveryError.unconfigured.localizedDescription, "B终端不能阻塞其他目标的检查")
        XCTAssertNil(model.deliveryErrors[two.id], "B终端期间不得尝试后台连接")
        model.endTerminalSession(machineID: b.projects[0].machineID, id: terminal)
        await model.deliverPending(machines: [], cloud: nil, vault: vault)
        XCTAssertEqual(model.deliveryErrors[one.id], ThreadDeliveryError.unconfigured.localizedDescription, "无项目应指向工作空间配置")
        XCTAssertEqual(model.deliveryErrors[two.id], CloudTextError.unauthorized.localizedDescription, "缺云授权不能误报成缺项目")
        XCTAssertFalse(model.deliveryInProgress, "失败后必须释放单次投递状态")
        let reopened = WorkspaceLibraryModel(store: store)
        await reopened.deliverPending(machines: [], cloud: nil, vault: vault)
        let pending = try await store.pendingThreadDeliveries()
        let historyA = try await store.messages(threadID: first.id)
        let historyB = try await store.messages(threadID: second.id)
        XCTAssertEqual(pending.map(\.id), original.map(\.id), "前台恢复与 Model 重建不能创建新操作")
        XCTAssertEqual(historyA.map(\.state), [.pending], "A 未获得回执必须待发")
        XCTAssertEqual(historyB.map(\.state), [.pending], "B 未获得回执必须待发")
        XCTAssertEqual(historyB.map(\.text), ["继续讨论"], "失败不得改写或丢失原文")
    }
}

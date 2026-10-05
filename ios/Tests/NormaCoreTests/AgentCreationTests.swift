import Foundation
import Testing
@testable import NormaCore

private actor CreationMac {
    var receipt: AgentCreationReceipt?
    var rejection: MobileError?
    func reject(_ error: MobileError?) { rejection = error }
    var creations = 0
    func create(_ request: WorkspaceAgentCreateRequest) throws -> AgentCreationReceipt {
        if let rejection { throw rejection }
        if let receipt, receipt.operationID == request.operationID { return receipt }
        creations += 1
        let value = "{\"mac_id\":\"mac\",\"device_id\":\"phone\",\"desktop_workspace_id\":\"space\",\"operation_id\":\"\(request.operationID)\",\"state\":\"succeeded\",\"result\":{\"area_id\":\"area\",\"definition_id\":3,\"name\":\"Adam\",\"profile\":\"Codex\",\"card\":{\"node_id\":7,\"profile\":\"Codex\",\"title\":\"Adam\",\"mobile_control\":true,\"state\":\"ready\"}}}"
        let result = try JSONDecoder().decode(AgentCreationReceipt.self, from: Data(value.utf8))
        receipt = result; return result
    }
}
private struct CreationAPI: WorkspaceChatAPI {
    let mac: CreationMac
    let loseReply: Bool
    var offline = false
    var capable = true
    func catalog() async throws -> WorkspaceCatalog {
        if offline { throw MobileError.connectionFailed }
        let catalog = try JSONDecoder().decode(WorkspaceCatalog.self, from: Data(#"{"mac_id":"mac","workspaces":[{"id":"space","name":"测试空间","revision":1,"projects":[{"area_id":"area","project_id":"project","name":"测试项目","root":"/tmp/project","agents":[],"definitions":[]}]}],"available_projects":[],"capabilities":["workspace_agent_create_v1"]}"#.utf8))
        return WorkspaceCatalog(macID: catalog.macID, workspaces: catalog.workspaces, availableProjects: [], capabilities: capable ? catalog.capabilities : nil)
    }
    func receipt(for message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt? { nil }
    func submit(_ message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt { throw MobileError.invalidResponse }
    func events(workspaceID: String, after: UInt64) async throws -> [WorkspaceChatEvent] { [] }
    func configure(workspaceID: String, revision: UInt64, change: WorkspaceConfigurationChange) async throws {}
    func creationReceipt(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt? { await mac.receipt.flatMap { $0.operationID == request.operationID ? $0 : nil } }
    func createAgent(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt {
        let result = try await mac.create(request)
        if loseReply { throw MobileError.timedOut }
        return result
    }
}

/// 场景：Mac 已创建但直连回复丢失，中继必须先查询原操作而非再次创建。
/// 数据构造：一个 Mac、两条连接；固定空间 space、区域 area；一次独立操作。
/// 执行过程：直连提交 → 丢回复 → 中继查回执 → 返回原定义3及卡片7。
/// 预期结果：创建次数为1；原身份保留；结果不是未接收状态。
@Test func creationTransportSwitchDoesNotDuplicateAgent() async throws {
    let mac = CreationMac()
    let client = WorkspaceChatClient(direct: CreationAPI(mac: mac, loseReply: true), relay: { CreationAPI(mac: mac, loseReply: false) })
    let request = WorkspaceAgentCreateRequest(workspaceID: "space", areaID: "area", profile: "Codex", revision: 1)
    let receipt = try await client.createAgent(request)
    #expect(await mac.creations == 1, "换连接不得创建第二个 Agent")
    #expect(receipt.operationID == request.operationID, "必须查询原创建操作")
    #expect(receipt.result?.card?.nodeID == 7, "必须返回实际创建的卡片7")
    #expect(receipt.state != "received", "已成功创建不能退回未接收")
}

/// 场景：未决创建跨重启保留，聊天草稿不被创建覆盖。
/// 数据构造：空间 space 的文字草稿与区域 area 的一次创建请求。
/// 执行过程：保存草稿 → 保存创建 → 重开数据库读取 → 查询其它区域。
/// 预期结果：原操作仍在且标为未决；草稿原文保留；其它区域没有待创建记录。
@Test func pendingCreationPreservesScopeAndChatDraft() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let url = directory.appendingPathComponent("chat.sqlite")
    let store = WorkspaceChatStore(databaseURL: url)
    let request = WorkspaceAgentCreateRequest(workspaceID: "space", areaID: "area", profile: "Codex", revision: 1)
    try await store.saveDraft("原来的聊天草稿", macID: "mac", workspaceID: "space")
    try await store.saveCreation(.init(macID: "mac", request: request))
    let reopened = WorkspaceChatStore(databaseURL: url)
    #expect(try await reopened.pendingCreation(macID: "mac", workspaceID: "space", areaID: "area")?.request.operationID == request.operationID, "重启必须继续核查原操作")
    #expect(try await reopened.pendingCreation(macID: "mac", workspaceID: "space", areaID: "other") == nil, "创建请求不能串项目")
    #expect(try await reopened.draft(macID: "mac", workspaceID: "space") == "原来的聊天草稿", "创建不得清空群聊输入")
}

/// 场景：明确拒绝创建不应留下永久未决状态，用户修复授权后可以再次主动创建。
/// 数据构造：临时数据库、1个固定真实目标，首次Mac回unauthorized，次次授权恢复。
/// 执行过程：创建被拒 → 核查没有未决 → 恢复授权 → 再主动运行两次。
/// 预期结果：拒绝没有创建；两次独立运行产生两个不同操作；不存在待发送聊天任务。
@Test func creationRejectionDoesNotBlockFutureInvocations() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let store = WorkspaceChatStore(databaseURL: directory.appendingPathComponent("chat.sqlite"))
    let service = AgentCreationService(store: store)
    let mac = CreationMac()
    let client = WorkspaceChatClient(direct: CreationAPI(mac: mac, loseReply: false), relay: { throw MobileError.connectionFailed })
    await mac.reject(.unauthorized)
    await #expect(throws: MobileError.unauthorized) { try await service.create(macID: "mac", workspaceID: "space", areaID: "area", profile: "Codex", client: client) }
    #expect(await mac.creations == 0, "明确拒绝不能创建卡片")
    #expect(try await store.pendingCreation(macID: "mac", workspaceID: "space", areaID: "area") == nil, "授权拒绝不是结果未知，不得永久阻塞")
    await mac.reject(nil)
    let first = try await service.create(macID: "mac", workspaceID: "space", areaID: "area", profile: "Codex", client: client)
    let second = try await service.create(macID: "mac", workspaceID: "space", areaID: "area", profile: "Codex", client: client)
    #expect(first.operationID != second.operationID, "两次独立主动运行必须各有操作身份")
    #expect(await mac.creations == 2, "两次主动运行必须新建两张卡")
    #expect(try await store.outbox(macID: "mac").isEmpty, "创建不能隐式派发聊天任务")
}

/// 场景：离线和旧Mac在提交前返回具体原因，不会形成自动创建队列。
/// 数据构造：独立临时数据库；同目标但分别让目录断线或省略能力标记。
/// 执行过程：主动创建 → 目录预检失败 → 查询未决创建及群聊待发送记录。
/// 预期结果：返回offline或unsupported；没有任何创建、待发送和待创建记录。
@Test(arguments: [true, false]) func creationPreflightNeverQueuesWhenOfflineOrUnsupported(offline: Bool) async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let store = WorkspaceChatStore(databaseURL: directory.appendingPathComponent("chat.sqlite"))
    let mac = CreationMac()
    let api = CreationAPI(mac: mac, loseReply: false, offline: offline, capable: false)
    let client = WorkspaceChatClient(direct: api, relay: { throw MobileError.connectionFailed })
    let service = AgentCreationService(store: store)
    await #expect(throws: offline ? AgentCreationError.offline : AgentCreationError.unsupported) { try await service.create(macID: "mac", workspaceID: "space", areaID: "area", profile: "Codex", client: client) }
    #expect(await mac.creations == 0, "目录预检失败不能产生Mac卡片")
    #expect(try await store.pendingCreation(macID: "mac", workspaceID: "space", areaID: "area") == nil, "不允许离线排队，重连后也不应自动创建")
    #expect(try await store.pending(macID: "mac").isEmpty, "创建不能进入群聊派工队列")
}

/// 场景：两条连接都丢失返回时，下一次主动运行必须先恢复旧操作。
/// 数据构造：直连真实创建一次后超时；中继不可连；本地SQLite保留请求。
/// 执行过程：创建返回未决 → 读取原操作 → 重建服务 → 查到原成功回执。
/// 预期结果：操作ID相同、只创建一次；恢复后没有未决状态，不会多创建卡片。
@Test func uncertainCreationAfterServiceRestartQueriesOriginalOperation() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let store = WorkspaceChatStore(databaseURL: directory.appendingPathComponent("chat.sqlite"))
    let mac = CreationMac()
    let client = WorkspaceChatClient(direct: CreationAPI(mac: mac, loseReply: true), relay: { throw MobileError.connectionFailed })
    await #expect(throws: AgentCreationError.unresolved) { try await AgentCreationService(store: store).create(macID: "mac", workspaceID: "space", areaID: "area", profile: "Codex", client: client) }
    let pending = try #require(try await store.pendingCreation(macID: "mac", workspaceID: "space", areaID: "area"))
    let restored = try await AgentCreationService(store: store).create(macID: "mac", workspaceID: "space", areaID: "area", profile: "Claude", client: client)
    #expect(restored.result?.profile == "Codex", "未决恢复不能按用户新选的类型另建Claude")
    #expect(restored.operationID == pending.request.operationID, "重启恢复必须沿用原操作")
    #expect(await mac.creations == 1, "结果核查不得再创建一次")
    #expect(try await store.pendingCreation(macID: "mac", workspaceID: "space", areaID: "area") == nil, "已确认成功应解除未决状态")
}

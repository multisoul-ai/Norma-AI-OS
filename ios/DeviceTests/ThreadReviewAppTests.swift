import XCTest
import Security
import SQLite3
@testable import Norma
@testable import NormaCore

private struct ReviewUIFixture: Decodable {
    let port: UInt16
    let namespace: String
    let machineID: String
    let projectID: String
    let endpoints: [URL]
    let message: ThreadMessageOperation
    let task: ThreadTaskOperation
    let cloud: CloudDeviceCredential
    let mac: DeviceCredential
    var directory: URL { URL.applicationSupportDirectory.appendingPathComponent("Norma/UITestWorkspaces/\(namespace)") }
    var cloudService: String { "com.norma-ai.os.ios.cloud.local.\(port).\(namespace)" }
    static func load() throws -> Self {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "thread-review-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_THREAD_REVIEW_FIXTURE"],
              FileManager.default.fileExists(atPath: path) else { throw XCTSkip("需要正式本机中继的显式 UI fixture") }
        let fixture = try JSONDecoder().decode(Self.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        guard UUID(uuidString: fixture.namespace) != nil else { throw MobileError.invalidResponse }
        return fixture
    }
}

final class ThreadReviewAppTests: XCTestCase {
    /// 为成果 UI 准备隔离线程，并从正式中继查询真实 Mac 回执；产品代码无 fixture 入口。
    /// 数据构造：1 个 Node 已送达的来源/任务，1 台临时 Mac，正文140KB；namespace随机UUID。
    /// 执行过程：拒绝已有测试库/凭据 → 保存隔离身份 → 同步原消息和任务 → 检查唯一回复。
    /// 预期结果：手机状态全部来自当前 Mac 服务；没有新派工，原任务不进入发送队列。
    func testPrepareReviewUI() async throws {
        let fixture = try ReviewUIFixture.load()
        let vault = CredentialVault()
        let existing = try await vault.load(machineID: fixture.machineID)
        XCTAssertNil(existing, "不得覆盖已有 Mac 配对")
        guard existing == nil, !FileManager.default.fileExists(atPath: fixture.directory.path) else {
            XCTFail("必须使用全新的隔离 UI fixture"); return
        }
        let credentialQuery: [CFString: Any] = [kSecClass: kSecClassGenericPassword,
            kSecAttrService: "com.norma-ai.os.ios.device", kSecAttrAccount: fixture.machineID,
            kSecAttrSynchronizable: false, kSecAttrAccessible: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            kSecValueData: try JSONEncoder().encode(fixture.mac)]
        XCTAssertEqual(SecItemAdd(credentialQuery as CFDictionary, nil), errSecSuccess, "只添加本次隔离配对凭据")
        let machine = try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: [
            "id": fixture.machineID, "name": "成果验收 Mac（测试）", "certificateSHA256": String(fixture.machineID.dropFirst(4)),
            "endpoints": fixture.endpoints.map(\.absoluteString)]))
        let machineStore = MachineStore(url: URL.applicationSupportDirectory.appendingPathComponent("Norma/machines.json"))
        try await machineStore.save(machine)
        try await CloudCredentialVault(service: fixture.cloudService).save(fixture.cloud)
        let store = CollaborationWorkspaceStore(databaseURL: fixture.directory.appendingPathComponent("workspaces.sqlite"))
        try await store.seedReviewUI(fixture)
        let connection = try await CollaborationRelayClient(localPort: fixture.port).connect(machine, cloud: fixture.cloud, vault: vault)
        do {
            let source = try await connection.receiveThreadMessage(fixture.message)
            try await store.applyThreadReceipt(source)
            let target = WorkspaceAgentReference(machineID: fixture.machineID, projectID: fixture.projectID, definitionID: 2, name: "开发", profile: "Codex")
            let queued = try await store.enqueueThreadTask(sourceMessageID: fixture.message.messageID, operationID: fixture.task.operationID,
                target: target, task: fixture.task.task, contextVersion: fixture.task.contextVersion, context: fixture.task.context, revisionOf: fixture.task.revisionOf)
            XCTAssertEqual(queued.task, fixture.task, "本机操作必须与 Mac 已保存的原任务一致")
            let result = try await connection.threadTaskReceipt(for: fixture.task)
            try await store.applyThreadTaskReceipt(XCTUnwrap(result), deviceID: fixture.mac.deviceID)
            let pending = try await store.pendingThreadTasks(includingExecution: true)
            let messages = try await store.messages(threadID: fixture.task.threadID)
            XCTAssertTrue(pending.isEmpty, "已完成成果不得触发重复派工")
            XCTAssertEqual(messages.map(\.author), ["user", "agent"], "回执只产生一条原 Agent 回复")
            await connection.close()
        } catch { await connection.close(); throw error }
    }

    /// UI 验收结束后只清理本次临时 Mac/云凭据，保留用户其他电脑和隔离证据库。
    /// 数据构造：当前fixture的随机Mac身份及namespace，精确定位两条凭据。
    /// 执行过程：删除指定配对和云凭据 → 查询确认；不扫描或清空用户Keychain。
    /// 预期结果：该临时Mac不再存在，其余记录由MachineStore按ID保留。
    func testCleanupReviewUI() async throws {
        let fixture = try ReviewUIFixture.load()
        let store = MachineStore(url: URL.applicationSupportDirectory.appendingPathComponent("Norma/machines.json"))
        let before = try await store.load()
        try await store.remove(id: fixture.machineID)
        let vault = CredentialVault()
        try await vault.remove(machineID: fixture.machineID)
        let cloud = CloudCredentialVault(service: fixture.cloudService)
        try await cloud.remove()
        let remaining = try await store.load()
        XCTAssertEqual(remaining, before.filter { $0.id != fixture.machineID }, "必须保留其他电脑元数据")
        let macCredential = try await vault.load(machineID: fixture.machineID)
        let cloudCredential = try await cloud.load()
        XCTAssertNil(macCredential, "临时配对不能残留")
        XCTAssertNil(cloudCredential, "临时云准入不能残留")
    }
}

private extension CollaborationWorkspaceStore {
    func seedReviewUI(_ fixture: ReviewUIFixture) throws {
        let source = fixture.message
        let workspace = CollaborationWorkspace(id: source.collaborationWorkspaceID, name: "成果验收（测试）",
            projects: [.init(machineID: fixture.machineID, projectID: fixture.projectID, name: "iOS", machineName: "验收 Mac")],
            agents: [.init(machineID: fixture.machineID, projectID: fixture.projectID, definitionID: 2, name: "开发", profile: "Codex")],
            archived: false, createdAt: .now, updatedAt: .now)
        let thread = WorkThread(id: source.threadID, workspaceID: workspace.id, title: "首屏导航", sourceIdeaID: nil,
            sourceExcerpt: nil, originDeviceID: source.originDeviceID, createdAt: .now)
        let message = WorkThreadMessage(id: source.messageID, workspaceID: workspace.id, threadID: thread.id,
            author: "user", originDeviceID: source.originDeviceID, text: source.text, createdAt: .now, state: .pending)
        let outbox = ThreadOutboxOperation(id: source.operationID, messageID: message.id,
            machineID: fixture.machineID, message: source, receipt: nil)
        try threadDatabase(writing: true) { db in
            let origin = try prepare(db, "INSERT INTO collaboration_origin(slot,id) VALUES(1,?)")
            defer { sqlite3_finalize(origin) }
            try bind(origin, [source.originDeviceID])
            guard sqlite3_step(origin) == SQLITE_DONE else { throw WorkThreadError.damagedStore }
            try insertThreadRecord(db, "INSERT INTO collaboration_workspaces(id,updated_at,configuration) VALUES(?,?,?)", [workspace.id, "0"], workspace)
            try insertThreadRecord(db, "INSERT INTO work_threads(id,workspace_id,payload) VALUES(?,?,?)", [thread.id, workspace.id], thread)
            try insertThreadRecord(db, "INSERT INTO work_messages(id,workspace_id,thread_id,payload) VALUES(?,?,?,?)", [message.id, workspace.id, thread.id], message)
            try insertThreadRecord(db, "INSERT INTO thread_outbox(id,message_id,payload) VALUES(?,?,?)", [outbox.id, message.id], outbox)
        }
    }
}

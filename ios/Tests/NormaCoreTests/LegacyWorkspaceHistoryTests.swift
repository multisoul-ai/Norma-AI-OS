import Foundation
import Testing
import SQLite3
@testable import NormaCore

/// 场景：查看不存在的旧记录仅返回空列表，不能创建旧数据库或待派发任务。
/// 数据构造：独立临时目录中尚未存在的 legacy.sqlite；新群聊与桌面配置完全独立。
/// 执行过程：构造历史读取器 → 查看空间列表 → 检查文件系统。
/// 预期结果：列表为空；旧数据库文件仍不存在，不能借读取恢复旧写入流程。
@Test func legacyHistoryReadDoesNotCreateOldDatabase() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let path = directory.appendingPathComponent("legacy.sqlite")
    let store = LegacyWorkspaceHistoryStore(databaseURL: path)
    #expect(try await store.all().isEmpty, "无历史时应返回空列表")
    #expect(!FileManager.default.fileExists(atPath: path.path), "只读兼容入口不得创建旧版数据库和写入流程")
}

/// 场景：升级后旧库的来源、草稿、待发送消息与任务仍可只读查看，不产生迁移副作用。
/// 数据构造：1个旧空间、1条来源想法线程、1条pending消息、1条待发任务、1份草稿；另一线程无记录。
/// 执行过程：构造旧格式SQLite → 保存数据库原字节 → 读取六种历史 → 核对原文、隔离与文件字节。
/// 预期结果：待发记录状态不改变；草稿/任务正文不丢；另一线程为空；数据库内容一字节也不变。
@Test func existingLegacyHistoryPreservesPendingTextAndDatabaseBytes() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    let path = directory.appendingPathComponent("legacy.sqlite")
    let workspaceID = UUID().uuidString, threadID = UUID().uuidString, messageID = UUID().uuidString
    let operationID = UUID().uuidString, taskID = UUID().uuidString, ideaID = UUID().uuidString
    let oldWorkspace = CollaborationWorkspace(id: workspaceID, name: "同名桌面空间", projects: [], agents: [], archived: false, createdAt: .distantPast, updatedAt: .distantPast)
    let thread = WorkThread(id: threadID, workspaceID: workspaceID, title: "原线程", sourceIdeaID: ideaID,
                            sourceExcerpt: "独立来源原文", originDeviceID: "phone", createdAt: .distantPast)
    let message = WorkThreadMessage(id: messageID, workspaceID: workspaceID, threadID: threadID,
        author: "user", originDeviceID: "phone", text: "不要自动重发的消息", createdAt: .distantPast, state: .pending)
    let draft = WorkThreadDraft(text: "尚未发出的草稿")
    let delivery = ThreadOutboxOperation(id: operationID, messageID: messageID)
    let task = ThreadTaskOperation(operationID: taskID, collaborationWorkspaceID: workspaceID, threadID: threadID,
        sourceOperationID: operationID, sourceMessageID: messageID, projectID: "old-project", agentDefinitionID: 1,
        task: "旧任务原文", contextVersion: 1, context: "旧上下文")
    let pendingTask = ThreadTaskOutboxOperation(machineID: "old-mac", task: task)
    var db: OpaquePointer?
    #expect(sqlite3_open(path.path, &db) == SQLITE_OK, "测试旧库必须可创建")
    let connection = try #require(db)
    let schema = """
    CREATE TABLE collaboration_workspaces(id TEXT, configuration BLOB, updated_at REAL);
    CREATE TABLE work_threads(id TEXT, workspace_id TEXT, payload BLOB);
    CREATE TABLE work_messages(id TEXT, thread_id TEXT, payload BLOB);
    CREATE TABLE work_thread_drafts(thread_id TEXT, payload BLOB);
    CREATE TABLE thread_outbox(id TEXT, payload BLOB);
    CREATE TABLE thread_task_outbox(id TEXT, thread_id TEXT, payload BLOB);
    """
    #expect(sqlite3_exec(connection, schema, nil, nil, nil) == SQLITE_OK, "构造真实旧表结构")
    func insert<T: Encodable>(_ sql: String, _ value: T) throws {
        let data = try JSONEncoder().encode(value)
        var stmt: OpaquePointer?
        #expect(sqlite3_prepare_v2(connection, sql, -1, &stmt, nil) == SQLITE_OK, "测试数据应可绑定写入")
        defer { sqlite3_finalize(stmt) }
        let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        let bound = data.withUnsafeBytes { sqlite3_bind_blob(stmt, 1, $0.baseAddress, Int32(data.count), transient) }
        #expect(bound == SQLITE_OK, "旧JSON原文必须完整写入")
        #expect(sqlite3_step(stmt) == SQLITE_DONE, "旧记录插入应完成")
    }
    try insert("INSERT INTO collaboration_workspaces VALUES ('\(workspaceID)',?,0)", oldWorkspace)
    try insert("INSERT INTO work_threads VALUES ('\(threadID)','\(workspaceID)',?)", thread)
    try insert("INSERT INTO work_messages VALUES ('\(messageID)','\(threadID)',?)", message)
    try insert("INSERT INTO work_thread_drafts VALUES ('\(threadID)',?)", draft)
    try insert("INSERT INTO thread_outbox VALUES ('\(operationID)',?)", delivery)
    try insert("INSERT INTO thread_task_outbox VALUES ('\(taskID)','\(threadID)',?)", pendingTask)
    #expect(sqlite3_close(connection) == SQLITE_OK, "构造完成后关闭写连接")
    let original = try Data(contentsOf: path)
    let store = LegacyWorkspaceHistoryStore(databaseURL: path)
    #expect(try await store.all().first?.id == workspaceID, "同名旧空间仍保留原ID，不匹配桌面空间")
    #expect(try await store.threads(workspaceID: workspaceID).first?.sourceExcerpt == "独立来源原文", "想法来源必须保留")
    #expect(try await store.messages(threadID: threadID).first?.text == "不要自动重发的消息", "待发消息原文不能丢失")
    #expect(try await store.messages(threadID: threadID).first?.state == .pending, "历史读取不能把待发记录变成已送达")
    #expect(try await store.messageDraft(threadID: threadID)?.text == "尚未发出的草稿", "未提交草稿应可查看")
    #expect(try await store.legacyThreadDeliveries(threadID: threadID).first?.receipt == nil, "读取不得伪造回执")
    #expect(try await store.legacyThreadTasks(threadID: threadID).first?.task.task == "旧任务原文", "旧任务必须完整保留")
    #expect(try await store.messages(threadID: UUID().uuidString).isEmpty, "其他线程不能读到这些消息")
    #expect(try Data(contentsOf: path) == original, "只读历史不得迁移或修改旧数据库")
}

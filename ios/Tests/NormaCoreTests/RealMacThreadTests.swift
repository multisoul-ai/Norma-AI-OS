import Foundation
import XCTest
@testable import NormaCore

@MainActor final class RealMacThreadTests: XCTestCase {
    /// 已登录的真实 Codex 经 Mac App 执行手机线程任务，回复与冻结文件回原线程。
    /// 数据构造：独立 Mac/项目/手机授权，1 个 Workspace、2 个线程、1 个执行指令；无模型替身。
    /// 执行过程：持久讨论送达→派工→断开重连→等真实完成→重复派工→读取成果→重建手机 Store。
    /// 预算：180秒=CLI启动30秒+模型/文件执行120秒+Hook/冻结成果30秒；每2秒最多一次查询。
    /// 预期结果：result.txt准确，executions.txt仅一行；原线程1条Agent回复，其他线程无回复；无自动验收。
    func testRealCodexExecutionReturnsToOriginalThread() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_THREAD_REAL_FIXTURE"] else {
            throw XCTSkip("需显式启动隔离真实 Mac App、本地中继与已登录 Codex")
        }
        struct Fixture: Decodable {
            let port: UInt16
            let machineID: String
            let projectID: String
            let definitionID: Int64
            let root: String
            let cloud: CloudDeviceCredential
            let mac: DeviceCredential
        }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let root = URL(fileURLWithPath: fixture.root)
        let database = root.appendingPathComponent("phone.sqlite")
        XCTAssertFalse(FileManager.default.fileExists(atPath: database.path), "每次真实模型测试必须使用新手机库，避免重复执行")
        guard !FileManager.default.fileExists(atPath: database.path) else { return }
        let client = try CollaborationRelayClient(localPort: fixture.port)
        let store = CollaborationWorkspaceStore(databaseURL: database)
        var workspace = try await store.create(name: "真实 Mac 派工验收")
        let target = WorkspaceAgentReference(machineID: fixture.machineID, projectID: fixture.projectID,
            definitionID: fixture.definitionID, name: "Alice", profile: "Codex")
        workspace.projects = [.init(machineID: fixture.machineID, projectID: fixture.projectID, name: "Codex L3", machineName: "隔离 Mac App")]
        workspace.agents = [target]
        try await store.save(workspace)
        let thread = try await store.createThread(workspaceID: workspace.id, title: "真实写入两份文件")
        let other = try await store.createThread(workspaceID: workspace.id, title: "不应收到成果")
        let instruction = "在当前项目写入 result.txt，完整内容为 norma-real-codex 加一个换行；向 executions.txt 追加一行 executed（不存在时创建）。只修改这两个文件。完成后回复 NORMA_REAL_CODEX_OK，不要提交 git。"
        let message = try await store.appendMessage(threadID: thread.id, id: UUID().uuidString, text: instruction)
        let task = try await store.enqueueThreadTask(sourceMessageID: message.id, operationID: UUID().uuidString,
            target: target, task: instruction, contextVersion: 1, context: "这是隔离的本地验收项目。")
        // Mac registers every2s; 25×200ms=5s allows two registration retries before failing.
        var connection: CollaborationRelayConnection?
        for attempt in 0..<25 {
            do { connection = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac); break }
            catch { if attempt == 24 { throw error }; try await Task.sleep(for: .milliseconds(200)) }
        }
        let first = try XCTUnwrap(connection)
        do {
            let received = try await store.deliverThreadMessage(messageID: message.id, using: first)
            XCTAssertEqual(received.state, "received", "真实 Mac 持久接收才算送达")
            let dispatched = try await store.deliverThreadTask(operationID: task.id, using: first)
            XCTAssertNotEqual(dispatched.receipt.state, "rejected", "真实 App 应接受已启用 Agent：\(String(describing: dispatched.receipt.error))")
            await first.close()
        } catch { await first.close(); throw error }
        let resumed = try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        do {
            let deadline = Date().addingTimeInterval(180)
            var receipt = try await store.deliverThreadTask(operationID: task.id, using: resumed)
            while Date() < deadline && (receipt.execution?.execution.terminal != true || receipt.execution?.execution.delivery == nil) {
                try await Task.sleep(for: .seconds(2))
                receipt = try await store.deliverThreadTask(operationID: task.id, using: resumed)
            }
            let execution = try XCTUnwrap(receipt.execution?.execution, "必须获取真实执行状态")
            XCTAssertEqual(execution.state, "done", "真实任务必须完成：\(execution.error ?? receipt.receipt.error ?? "无错误描述")")
            XCTAssertTrue(execution.result?.text?.contains("NORMA_REAL_CODEX_OK") == true, "必须获得真实 Codex 最终回复")
            let snapshot = try await resumed.snapshot(workspaceID: fixture.projectID)
            let project = try XCTUnwrap(snapshot.projects.first)
            let route = try XCTUnwrap(project.routeID)
            let replay = try await resumed.submitThreadTask(task.task, routeID: route, grantRevision: snapshot.grant.revision)
            XCTAssertEqual(replay.receipt.result?.taskRef, receipt.receipt.result?.taskRef, "重复提交只能取回原任务身份")
            let wrongThread = ThreadTaskOperation(operationID: task.task.operationID,
                collaborationWorkspaceID: task.task.collaborationWorkspaceID, threadID: other.id,
                sourceOperationID: task.task.sourceOperationID, sourceMessageID: task.task.sourceMessageID,
                projectID: task.task.projectID, agentDefinitionID: task.task.agentDefinitionID,
                task: task.task.task, contextVersion: task.task.contextVersion, context: task.task.context)
            let wrong = try await resumed.threadTaskReceipt(for: wrongThread)
            XCTAssertNil(wrong, "另一线程不得读到该任务")
            let review = try await resumed.threadTaskReview(task: task.task, receipt: receipt)
            let result = try XCTUnwrap(review.review.files.first { $0.path == "result.txt" })
            let count = try XCTUnwrap(review.review.files.first { $0.path == "executions.txt" })
            XCTAssertEqual(String(bytes: try XCTUnwrap(result.after).bytes, encoding: .utf8), "norma-real-codex\n", "必须通过冻结成果接口读取真实文件")
            XCTAssertEqual(String(bytes: try XCTUnwrap(count.after).bytes, encoding: .utf8), "executed\n", "重连与重试不能再次执行文件追加")
            XCTAssertEqual(project.cardCount, 1, "一次派工只激活一个真实 Agent 卡片")
            let reopened = CollaborationWorkspaceStore(databaseURL: database)
            let replies = try await reopened.messages(threadID: thread.id).filter { $0.author == "agent" }
            XCTAssertEqual(replies.count, 1, "重建手机 Store 后只有一条原 Agent 回复")
            XCTAssertTrue(replies.first?.text.contains("NORMA_REAL_CODEX_OK") == true, "回复必须长期保留在原线程")
            let otherMessages = try await reopened.messages(threadID: other.id)
            XCTAssertTrue(otherMessages.isEmpty, "其他线程不能出现用户指令或 Agent 回复")
            let restored = try await reopened.threadTask(operationID: task.id)
            XCTAssertNil(restored.acceptance, "真实执行完成不能自动代表用户验收")
            try JSONEncoder().encode(receipt).write(to: root.appendingPathComponent("real-execution-receipt.json"), options: .atomic)
            await resumed.close()
        } catch { await resumed.close(); throw error }
    }
}

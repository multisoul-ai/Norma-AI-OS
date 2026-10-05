import Foundation
import Testing
@testable import NormaCore

private actor DurableMacFixture {
    var receipt: WorkspaceChatReceipt?
    var executions = 0
    func receive(_ message: WorkspaceChatMessage) throws -> WorkspaceChatReceipt {
        if let receipt { return receipt }
        executions += 1
        let data = Data("{\"device_id\":\"phone\",\"mac_id\":\"mac\",\"desktop_workspace_id\":\"space\",\"operation_id\":\"\(message.operationID)\",\"state\":\"processing\",\"cursor\":1}".utf8)
        let saved = try JSONDecoder().decode(WorkspaceChatReceipt.self, from: data)
        receipt = saved; return saved
    }
}
private struct FixtureChatAPI: WorkspaceChatAPI {
    let mac: DurableMacFixture
    let loseReply: Bool
    func catalog() async throws -> WorkspaceCatalog { throw MobileError.connectionFailed }
    func receipt(for message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt? { await mac.receipt }
    func submit(_ message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt {
        let receipt = try await mac.receive(message)
        if loseReply { throw MobileError.timedOut }
        return receipt
    }
    func events(workspaceID: String, after: UInt64) async throws -> [WorkspaceChatEvent] { [] }
    func configure(workspaceID: String, revision: UInt64, change: WorkspaceConfigurationChange) async throws {}
}

/// 场景：直连写入成功但回执丢失后，换中继先查回执，不重复执行。
/// 数据构造：共用一个持久 Mac Fixture；两条 transport；同一稳定操作 ID。
/// 执行过程：直连查空 → 直连提交且丢回复 → 中继查已存在回执 → 返回原回执。
/// 预期结果：执行次数恰为1，操作ID不变，不能创建第二次提交。
@Test func uncertainDirectSendQueriesRelayBeforeRetry() async throws {
    let mac = DurableMacFixture()
    let direct = FixtureChatAPI(mac: mac, loseReply: true)
    let relay = FixtureChatAPI(mac: mac, loseReply: false)
    let client = WorkspaceChatClient(direct: direct, relay: { relay })
    let message = WorkspaceChatMessage(workspaceID: "space", text: "执行", intent: .execute)
    let receipt = try await client.deliver(message)
    #expect(await mac.executions == 1, "丢回执换连接不得重复执行")
    #expect(receipt.operationID == message.operationID, "必须返回同一操作的真实回执")
    #expect(receipt.state != "received", "不能把已经处理的操作当作未发送")
}

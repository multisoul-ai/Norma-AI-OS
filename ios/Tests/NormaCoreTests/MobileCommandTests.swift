import XCTest
@testable import NormaCore

final class MobileCommandTests: XCTestCase {
    /// 从项目定义创建卡片时，iOS 必须把稳定 definition_id 与用户看到的名称一并保存和发送。
    ///
    /// 数据构造：definition_id=7、Codex、名称 Alan；另构造 definition_id=0 的非法请求。
    /// 执行过程：构造未决命令 → JSON 往返 → 检查关联身份 → 尝试非法 ID。
    /// 预期结果：合法请求精确保留 definition_id；零 ID 被拒绝，不能退化为同名普通卡片。
    func testProjectDefinitionCardCreationPreservesStableIdentity() throws {
        let pending = try PendingMobileCommand(
            machineID: "mac-" + String(repeating: "a", count: 64),
            deviceID: "phone-a",
            workspaceID: "project-a",
            routeID: "route-a",
            revision: 2,
            profile: "Codex",
            title: "Alan",
            definitionID: 7,
            operationID: "create-definition-a"
        )
        let data = try JSONEncoder().encode(pending)
        let reopened = try JSONDecoder().decode(PendingMobileCommand.self, from: data)

        XCTAssertEqual(reopened, pending, "项目定义创建命令必须可持久化并精确恢复")
        if case let .createCard(profile, title, definitionID) = reopened.request.command {
            XCTAssertEqual(profile, "Codex", "创建必须保留项目定义的 Provider")
            XCTAssertEqual(title, "Alan", "创建必须保留用户选择时看到的定义名称")
            XCTAssertEqual(definitionID, 7, "创建必须使用稳定 definition_id，不能只靠可变名称猜测")
        } else {
            XCTFail("项目定义请求必须编码为 create_card")
        }
        XCTAssertTrue(String(decoding: data, as: UTF8.self).contains(#""definition_id":7"#), "线上命令必须发送 definition_id")
        XCTAssertThrowsError(
            try PendingMobileCommand(
                machineID: pending.machineID,
                deviceID: pending.deviceID,
                workspaceID: "project-a",
                routeID: "route-a",
                revision: 2,
                profile: "Codex",
                title: "Alan",
                definitionID: 0
            ),
            "零 definition_id 不能被当作普通同名卡片创建"
        )
    }

    /// 原生卡片可构造删除请求，但未知状态、缺少运行确认和未支持Provider不能通过。
    ///
    /// 数据构造：node=7的原生Claude空卡，mobile_control=false；32字节哈希编码为64字符机器身份。
    /// 执行过程：原生空卡构造并往返 → 改为unknown → 改为无回合thinking → 改为Cursor与Grok。
    /// 预期结果：原模式保持false且Claude/Cursor删除可保存；不就绪或未支持Grok目标被拒绝。
    func testNativeCardDeletionDoesNotRequireControlledMode() throws {
        let raw = #"{"node_id":7,"profile":"Claude","title":"旧卡","mobile_control":false,"state":"idle"}"#
        func command(_ json: String) throws -> PendingMobileCommand {
            let card = try JSONDecoder().decode(CardSnapshot.self, from: Data(json.utf8))
            return try PendingMobileCommand(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 1, deleting: card, confirmBusy: false)
        }
        let pending = try command(raw)
        XCTAssertEqual(try JSONDecoder().decode(PendingMobileCommand.self, from: JSONEncoder().encode(pending)), pending, "原生卡请求可完整保存")
        if case let .deleteCard(card, _) = pending.request.command { XCTAssertFalse(card.mobileControl, "不得把旧卡模式改成受控") }
        else { XCTFail("必须是删除卡片命令") }
        XCTAssertThrowsError(try command(raw.replacingOccurrences(of: "idle", with: "unknown")), "未知状态不可删除")
        XCTAssertThrowsError(try command(raw.replacingOccurrences(of: "idle", with: "thinking")), "忙碌卡不能绕过运行确认")
        XCTAssertNoThrow(try command(raw.replacingOccurrences(of: "Claude", with: "Cursor")), "Cursor 原生卡应能使用同一删除契约")
        XCTAssertThrowsError(try command(raw.replacingOccurrences(of: "Claude", with: "Grok")), "未支持Provider不可操作")
    }

    /// Cursor 卡片应复用既有创建与消息队列协议，不能在本机持久化时退化成未知 Provider。
    ///
    /// 数据构造：definition_id=10 的 Cursor「Chloe」，以及 node=12/run/session 的就绪 Cursor 卡片。
    /// 执行过程：构造并往返创建命令 → 为同一卡片构造排队消息 → 检查 JSON 中的 Provider 与稳定身份。
    /// 预期结果：两类命令均可保存；创建保留 definition_id，排队目标仍是 Cursor；不得改写成 Codex。
    func testCursorCardCreationAndQueuedMessagePreserveProviderIdentity() throws {
        let machine = "mac-" + String(repeating: "c", count: 64)
        let creation = try PendingMobileCommand(
            machineID: machine,
            deviceID: "phone-cursor",
            workspaceID: "project-a",
            routeID: "route-a",
            revision: 3,
            profile: "Cursor",
            title: "Chloe",
            definitionID: 10,
            operationID: "create-cursor"
        )
        let creationData = try JSONEncoder().encode(creation)
        let reopened = try JSONDecoder().decode(PendingMobileCommand.self, from: creationData)
        let card = try JSONDecoder().decode(
            CardSnapshot.self,
            from: Data(#"{"node_id":12,"profile":"Cursor","title":"Chloe","mobile_control":true,"state":"ready","run_id":"run-cursor","session_id":"session-cursor","turn_id":null}"#.utf8)
        )
        let queued = try PendingMobileCommand(
            machineID: machine,
            deviceID: "phone-cursor",
            workspaceID: "project-a",
            routeID: "route-a",
            revision: 3,
            queuing: card,
            text: "继续检查测试",
            operationID: "queue-cursor"
        )

        XCTAssertEqual(reopened, creation, "Cursor 创建命令必须可跨重启精确恢复")
        if case let .createCard(profile, title, definitionID) = reopened.request.command {
            XCTAssertEqual(profile, "Cursor", "创建请求不能把 Cursor 改写成其他 Provider")
            XCTAssertEqual(title, "Chloe", "创建请求必须保留项目 Agent 名称")
            XCTAssertEqual(definitionID, 10, "创建请求必须保留 Cursor 定义身份")
        } else {
            XCTFail("Cursor 创建必须编码为 create_card")
        }
        if case let .queueMessage(target, text) = queued.request.command {
            XCTAssertEqual(target.profile, "Cursor", "排队消息必须指向原 Cursor 卡片")
            XCTAssertEqual(text, "继续检查测试", "排队消息不得改写正文")
            XCTAssertNotEqual(target.profile, "Codex", "Cursor 目标不得退化为 Codex")
        } else {
            XCTFail("Cursor 忙碌消息必须编码为 queue_message")
        }
    }
    /// 删除记录绑定卡片快照和确认选项；回执不能把另一张卡的删除显示为成功。
    ///
    /// 数据构造：node=7、run-a/session-a/turn-a 的 Claude 思考中卡片；另一回执 node=8。
    /// 执行过程：无确认构造拒绝 → 明确确认并编码 → 解码往返 → 精确回执通过 → 另一节点拒绝。
    /// 预期结果：保留回合和确认，不生成 create_card；只接受本次删除的节点回执。
    func testDeletionPersistsExactSnapshotAndChecksReceiptNode() throws {
        let raw = #"{"node_id":7,"profile":"Claude","title":"修复登录","mobile_control":true,"state":"thinking","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","task_title":null}"#
        let card = try JSONDecoder().decode(CardSnapshot.self, from: Data(raw.utf8))
        func command(_ confirmed: Bool) throws -> PendingMobileCommand {
            try PendingMobileCommand(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 2, deleting: card, confirmBusy: confirmed, operationID: "delete-a")
        }
        XCTAssertThrowsError(try command(false), "忙碌删除需明确确认")
        let pending = try command(true)
        let data = try JSONEncoder().encode(pending)
        XCTAssertEqual(try JSONDecoder().decode(PendingMobileCommand.self, from: data), pending, "删除请求须完整往返")
        let text = String(decoding: data, as: UTF8.self)
        XCTAssertTrue(text.contains("delete_card"), "应编码删除类型")
        XCTAssertFalse(text.contains("create_card"), "不能悄悄变成创建操作")
        let receipt = #"{"device_id":"phone-a","operation_id":"delete-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7},"error":null}"#
        try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(receipt.utf8)).validate(for: pending)
        XCTAssertThrowsError(try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(receipt.replacingOccurrences(of: "\"node_id\":7", with: "\"node_id\":8").utf8)).validate(for: pending), "另一卡片回执必须拒绝")
    }

    /// 手机必须先保存创建指令的准确身份，重启只回查；同 ID 的不同内容不能覆盖未决记录。
    ///
    /// 数据构造：一台 mac- 加 64 位证书 hash、phone-a、project-a、route-a、op-a；第二操作只修改标题。
    /// 执行过程：保存 → 重开读取 → 同 ID 换标题 → 写坏文件 → 再次保存 → 检查原损坏数据仍在。
    /// 预期结果：唯一指令完整往返，没有 token；冲突和损坏报错，不静默清空或重放。
    func testPendingCommandSurvivesRestartAndRejectsConflicts() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("commands.json")
        let command = try PendingMobileCommand(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 1, profile: "Codex", title: "新卡", operationID: "op-a")
        // Each combining sequence is one grapheme but two Unicode scalars; 41 × 2 = 82 > Rust's 80-character limit.
        XCTAssertThrowsError(try PendingMobileCommand(machineID: command.machineID, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 1, profile: "Codex", title: String(repeating: "e\u{301}", count: 41)), "Swift 必须与 Rust 的字符边界一致")
        let store = PendingCommandStore(url: url)
        try await store.save(command)
        let reopened = PendingCommandStore(url: url)
        let saved = try await reopened.load()
        XCTAssertEqual(saved, [command], "重开必须保留原请求 ID、项目和窗口")
        XCTAssertFalse(String(decoding: try Data(contentsOf: url), as: UTF8.self).contains("token"), "未决文件不得携带秘密凭据")
        let changed = try PendingMobileCommand(machineID: command.machineID, deviceID: command.deviceID, workspaceID: "project-a", routeID: "route-a", revision: 1, profile: "Codex", title: "另一张", operationID: "op-a")
        do { try await reopened.save(changed); XCTFail("同 ID 不同请求必须冲突") } catch { }
        let unchanged = try await reopened.load()
        XCTAssertEqual(unchanged, [command], "冲突不能覆盖已发送的请求")
        try Data("broken".utf8).write(to: url)
        do { try await reopened.save(command); XCTFail("损坏不能被空列表覆盖") } catch { }
        XCTAssertEqual(try Data(contentsOf: url), Data("broken".utf8), "损坏记录必须保留便于恢复")
    }

    /// 回执必须关联设备、操作、项目和窗口；accepted 不得伪造卡片成功。
    ///
    /// 数据构造：op-a 成功创建 node_id=7，绑定 phone-a/project-a/route-a；逐项替换身份或状态。
    /// 执行过程：读取正确回执 → 错手机 / 项目 / 窗口 / 操作 → accepted 带结果 → 成功缺节点。
    /// 预期结果：只接受具体且一致的成功结果；错来源和不完整状态全部拒绝。
    func testReceiptValidatesExactCommandIdentityAndOutcome() throws {
        let command = try PendingMobileCommand(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 1, profile: "Claude", title: nil, operationID: "op-a")
        let raw = #"{"device_id":"phone-a","operation_id":"op-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7},"error":null}"#
        func read(_ text: String) throws -> MobileCommandReceipt {
            let receipt = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(text.utf8))
            try receipt.validate(for: command)
            return receipt
        }
        XCTAssertEqual(try read(raw).result?.nodeID, 7, "必须返回准确的卡片 ID")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: "phone-a", with: "phone-b")), "另一手机的回执不可认领")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: "project-a", with: "project-b")), "同名项目不能串线")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: "route-a", with: "route-b")), "重开的窗口不是原执行窗口")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: "op-a", with: "op-b")), "不同操作 ID 必须拒绝")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: "succeeded", with: "accepted")), "accepted 不能同时携带成功结果")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: #"{"node_id":7}"#, with: "null")), "成功必须有实际卡片 ID")
    }
}

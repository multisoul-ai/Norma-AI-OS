import XCTest
@testable import NormaCore

@MainActor final class ThreadTerminalNetworkTests: XCTestCase {
    /// Swift通过正式中继读原终端、接管、输入、重连查回执，再显式接管和交还。
    /// 数据构造：1台已配对Mac、1个已完成派工、200行（最新120+历史80）；唯一输入draft=5字节。
    /// 执行过程：查询未送达Claim → 重连明确接管 → 受控分页 → 输入 → 断线 → 查询原回执/拒绝旧权 → 新接管/交还。
    /// 预期结果：查无记录不执行；正文输入只一次；重连旧成功不授予输入权，新Claim才可读可输入。
    /// 层级：真实Swift/Node/Rust网络及SQLite回执，Rust前台是明确fixture，不代表真实PTY和UI。
    func testTypedControlThroughProductionRelayAndReconnect() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_NATIVE_TERMINAL_FIXTURE"] else {
            throw XCTSkip("由正式本机中继专项提供隔离fixture")
        }
        struct Fixture: Decodable {
            let port: UInt16
            let machineID: String
            let task: ThreadTaskOperation
            let cloud: CloudDeviceCredential
            let mac: DeviceCredential
        }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let client = try CollaborationRelayClient(localPort: fixture.port)
        func connect() async throws -> CollaborationRelayConnection {
            try await client.connect(machineID: fixture.machineID, cloud: fixture.cloud, credential: fixture.mac)
        }
        let initial = try await connect()
        let received = try await initial.threadTaskReceipt(for: fixture.task)
        let taskReceipt = try XCTUnwrap(received)
        let terminal = try await initial.threadTaskTerminal(task: fixture.task, receipt: taskReceipt)
        XCTAssertFalse(terminal.page.inputReady, "普通读取不得取得输入权")
        var operation = ThreadTaskOutboxOperation(machineID: fixture.machineID, task: fixture.task)
        operation.deviceID = fixture.mac.deviceID; operation.receipt = taskReceipt
        let claim = ThreadTerminalAction()
        operation.terminalOperation = .init(action: claim, key: terminal.key, draftRevision: nil)
        do { _ = try await initial.threadTerminalReceipt(for: operation); XCTFail("仅查询不能新建Claim") }
        catch { XCTAssertEqual(error as? MobileError, .commandNotFound, "Mac没有该操作必须准确报告") }
        let owner = try await connect()
        let claimed = try await finish(operation, on: owner, submit: true)
        XCTAssertEqual(claimed.result?.status, "controlled", "明确接管到达前台才可确认")
        operation.terminalOperation?.receipt = claimed
        let controlled = try await owner.controlledThreadTerminal(for: operation, claimOperationID: claim.operationID)
        XCTAssertTrue(controlled.page.inputReady, "当前连接须获Mac确认才允许输入")
        XCTAssertEqual(controlled.page.rows.last, "fixture-terminal-199", "正文通过正式加密网络返回")
        let history = try await owner.controlledThreadTerminal(for: operation, claimOperationID: claim.operationID, before: controlled)
        XCTAssertFalse(history.page.inputReady, "历史页面不可输入")
        XCTAssertEqual(history.page.rows.first, "fixture-terminal-0", "历史分页应保留最早行")
        let input = try ThreadTerminalAction(claimOperationID: claim.operationID, page: controlled.page, input: .text("draft"))
        operation.terminalOperation = .init(action: input, key: terminal.key, draftRevision: nil)
        let written = try await finish(operation, on: owner, submit: true)
        XCTAssertEqual(written.result?.status, "written", "只有Mac实际回执才确认输入")
        await owner.close()
        // Codable复原原操作；真实手机SQLite持久化与草稿清理由ThreadTerminalControlTests另行验证。
        let restored = try JSONDecoder().decode(ThreadTaskOutboxOperation.self, from: JSONEncoder().encode(operation))
        let recovery = try await connect()
        let replay = try await recovery.threadTerminalReceipt(for: restored)
        XCTAssertEqual(replay, written, "恢复必须查到同一原回执，不能重复输入")
        do {
            _ = try await recovery.controlledThreadTerminal(for: restored, claimOperationID: claim.operationID)
            XCTFail("重连后的旧Claim不应恢复控制")
        } catch { XCTAssertEqual(error as? MobileError, .terminalControlExpired, "需要明确重新接管") }
        let newOwner = try await connect()
        let newClaim = ThreadTerminalAction()
        operation.terminalOperation = .init(action: newClaim, key: terminal.key, draftRevision: nil)
        _ = try await finish(operation, on: newOwner, submit: true)
        let fresh = try await newOwner.controlledThreadTerminal(for: operation, claimOperationID: newClaim.operationID)
        XCTAssertTrue(fresh.page.inputReady, "新明确接管应取得新输入权")
        operation.terminalOperation = .init(action: .release(claimOperationID: newClaim.operationID), key: terminal.key, draftRevision: nil)
        let released = try await finish(operation, on: newOwner, submit: true)
        XCTAssertEqual(released.result?.status, "released", "交还须取得真实回执")
        do {
            _ = try await newOwner.controlledThreadTerminal(for: operation, claimOperationID: newClaim.operationID)
            XCTFail("交还后旧读取不能再次取得输入权")
        } catch { XCTAssertEqual(error as? MobileError, .terminalControlExpired, "已释放输入权不可续期") }
        await newOwner.close()
    }

    private func finish(_ operation: ThreadTaskOutboxOperation, on connection: CollaborationRelayConnection,
                        submit: Bool) async throws -> MobileCommandReceipt {
        var receipt = try await (submit ? connection.submitThreadTerminalAction(for: operation) : connection.threadTerminalReceipt(for: operation))
        //20次×50ms=1s：只等待已接受操作的fixture完成，不发送第二次副作用。
        for _ in 0..<20 {
            if receipt.state != "accepted" { return receipt }
            try await Task.sleep(for: .milliseconds(50))
            receipt = try await connection.threadTerminalReceipt(for: operation)
        }
        throw MobileError.timedOut
    }
}

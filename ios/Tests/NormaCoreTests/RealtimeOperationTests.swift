import XCTest
@testable import NormaCore

@MainActor final class RealtimeOperationTests: XCTestCase {
    /// 豆包操作与GPT使用同一恢复契约，原媒体身份必须跨持久化保留。
    /// 数据构造：sequence=1表示首操作，doubao_A/rt-a初始无回执；task_id=41为执行层原任务。
    /// 执行过程：保存豆包未知操作 → 重开store → 用准确accepted回执核对 → 再重开读取。
    /// 预期结果：原call/operation/task均保留且仅一条记录；不能标记执行完成或接受空/异类媒体ID。
    func testDoubaoOperationRecoveryUsesBindingIdentityContract() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("operations.json")
        func operation(_ callID: String) -> RealtimeOperation {
            RealtimeOperation(sequence: 1, deviceID: "phone-a", callID: callID, turnID: "turn-a",
                              workspaceID: "project-a", operationID: "rt-a", result: nil)
        }
        XCTAssertNoThrow(try operation("doubao_A").validate(), "真实豆包会话必须能进入操作恢复")
        let record = try RealtimeOperationObservation(machineID: "mac-" + String(repeating: "a", count: 64),
                                                      operation: operation("doubao_A"))
        try await RealtimeOperationStore(url: url).save(record)
        let restored = try await RealtimeOperationStore(url: url).load()
        XCTAssertEqual(restored.count, 1, "首次恢复只能读取原操作，不得复制")
        XCTAssertEqual(restored.first?.operation.callID, "doubao_A", "恢复不能改成GPT的rtc身份")
        XCTAssertTrue(try XCTUnwrap(restored.first).unresolved, "尚无执行层回执，必须保持未知")
        let receipt = Data("""
        {"device_id":"phone-a","operation_id":"rt-a","workspace_id":"project-a","route_id":"route-a",
        "state":"succeeded","result":{"status":"accepted","task_id":41}}
        """.utf8)
        let verified = try record.applying(receipt: receipt)
        try await RealtimeOperationStore(url: url).save(verified)
        let settled = try await RealtimeOperationStore(url: url).load()
        XCTAssertEqual(settled.count, 1, "核对后不能新增第二个操作")
        XCTAssertEqual(settled.first?.operation.operationID, "rt-a", "必须更新原操作")
        XCTAssertEqual(settled.first?.operation.result?.taskID, 41, "不能重派成另一任务")
        XCTAssertEqual(settled.first?.label, "任务已接收", "accepted只表示任务接收")
        XCTAssertFalse(try XCTUnwrap(settled.first).unresolved, "原回执已核对，应解除未知状态")
        XCTAssertNotEqual(settled.first?.label, "执行完成", "外层succeeded不能冒充任务完成")
        XCTAssertNoThrow(try operation("rtc_A").validate(), "GPT原有恢复必须兼容")
        XCTAssertThrowsError(try operation("doubao_").validate(), "豆包空后缀不是会话身份")
        XCTAssertThrowsError(try operation("rtc_").validate(), "GPT空后缀不是会话身份")
        XCTAssertThrowsError(try operation("other_A").validate(), "未知媒体命名空间不能混入恢复记录")
    }

    /// 操作只读查询独立恢复短暂断网，错误身份及持续故障不能无限重试。
    /// 数据构造：第一次connectionFailed后返回原回执；持续unavailable；invalidCredential各一个探针。
    ///   重试等待1+2+4=7秒，最多首次1次+恢复3次=4次读取；不包含任何submit命令。
    /// 执行过程：短暂断网恢复 → 持续故障耗尽预算 → 错凭据直接失败。
    /// 预期结果：短暂故障返回原内容且只读两次；持续故障恰好四次；错凭据仅一次，不报成功。
    func testReadRecoveryIsBoundedAndRejectsCredentialErrors() async throws {
        let transient = ReadProbe(failures: 1, error: .connectionFailed)
        let restored = try await RealtimeBindingClient.readWithRecovery { try await transient.read() }
        let recoveredCalls = await transient.calls
        XCTAssertEqual(restored, Data("original receipt".utf8), "恢复必须返回原查询结果")
        XCTAssertEqual(recoveredCalls, 2, "第一次失败后独立恢复一次，不重建媒体")
        let unavailable = ReadProbe(failures: 4, error: .unavailable)
        do {
            _ = try await RealtimeBindingClient.readWithRecovery { try await unavailable.read() }
            XCTFail("连续四次离线必须返回失败，不能凭空返回回执")
        } catch {
            XCTAssertEqual(error as? MobileError, .unavailable, "耗尽重试后保留真实网络错误")
        }
        let unavailableCalls = await unavailable.calls
        XCTAssertEqual(unavailableCalls, 4, "最多三次恢复，不允许第五次读取")
        let rejected = ReadProbe(failures: 1, error: .invalidCredential)
        do {
            _ = try await RealtimeBindingClient.readWithRecovery { try await rejected.read() }
            XCTFail("鉴权失败不得用自动重试升级为成功")
        } catch {
            XCTAssertEqual(error as? MobileError, .invalidCredential, "错误凭据必须原样上报")
        }
        let rejectedCalls = await rejected.calls
        XCTAssertEqual(rejectedCalls, 1, "安全校验失败不能自动重试")
    }

    /// 手机保存未确认的操作身份，重启只按原电脑/项目查回执，不能把工具接收等同完成。
    /// 数据构造：sequence=1、同一rtc_A/turn-a/function映射operation；result=null表示执行结果未核对。
    /// 执行过程：解析Mac记录 → 保存 → 重开store → 以原操作解析accepted回执 → 注入错项目。
    /// 预期结果：原身份不丢失，工具accepted显示任务已接收；错归属拒绝，未知不升级成功。
    func testOperationObservationPersistsAndRequiresExactReceipt() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("operations.json")
        let operation = try JSONDecoder().decode(RealtimeOperation.self, from: Data("""
        {"sequence":1,"device_id":"phone-a","call_id":"rtc_A","turn_id":"turn-a",
        "workspace_id":"project-a","operation_id":"rt-a","result":null}
        """.utf8))
        let record = try RealtimeOperationObservation(machineID: "mac-" + String(repeating: "a", count: 64), operation: operation)
        try await RealtimeOperationStore(url: url).save(record)
        let restored = try await RealtimeOperationStore(url: url).load()
        XCTAssertEqual(restored.first?.operation.operationID, "rt-a", "重启保留原操作身份")
        XCTAssertEqual(restored.first?.label, "执行结果待核对", "无回执不能称为已接收")
        var receipt: [String: Any] = ["device_id":"phone-a", "operation_id":"rt-a", "workspace_id":"project-a",
            "route_id":"route-a", "state":"succeeded", "result":["status":"accepted","message":"已接收","task_id":41]]
        let verified = try record.applying(receipt: JSONSerialization.data(withJSONObject: receipt))
        XCTAssertEqual(verified.label, "任务已接收", "只说明执行层接收，不是任务完成")
        XCTAssertNotEqual(verified.label, "执行完成", "不能误用外层succeeded作为任务完成")
        receipt["workspace_id"] = "project-b"
        XCTAssertThrowsError(try record.applying(receipt: JSONSerialization.data(withJSONObject: receipt)), "另一项目不能覆盖原回执")
        XCTAssertNil(record.operation.result, "解析坏回执不能污染原未知状态")
    }
}

private actor ReadProbe {
    private(set) var calls = 0
    let failures: Int
    let error: MobileError
    init(failures: Int, error: MobileError) { self.failures = failures; self.error = error }
    func read() throws -> Data {
        calls += 1
        if calls <= failures { throw error }
        return Data("original receipt".utf8)
    }
}

import XCTest
@testable import NormaCore

final class ThreadReviewTests: XCTestCase {
    /// 大成果与自动投递共用每Mac预算，切换连接不能重置可用帧数。
    /// 数据构造：服务120帧/分钟；客户端上限108，留12帧给注册/关闭等控制流。
    /// 执行过程：t=0占满108 → t=0/30尝试多1帧 → t=60旧帧过期 → 新连接占3帧。
    /// 预期结果：超限返回60/30秒等待，不提前占位；满窗口过后只保留3帧，其他Mac有独立预算。
    func testReviewAndBackgroundTrafficShareARollingFrameBudget() {
        var budget = RelayFrameBudget()
        XCTAssertEqual(budget.reserve(at: 0, frames: 108), 0, "窗口内可保守使用108帧")
        XCTAssertEqual(budget.reserve(at: 0, frames: 1), 60, "第109帧应等待窗口，不允许突发断线")
        XCTAssertEqual(budget.reserve(at: 30, frames: 1), 30, "失败预留不能移动原窗口")
        XCTAssertEqual(budget.reserve(at: 60, frames: 3), 0, "旧帧过期后连接握手能继续")
        XCTAssertEqual(budget.reserve(at: 60, frames: 105), 0, "剩余额度为108-3=105")
        XCTAssertGreaterThan(budget.reserve(at: 60, frames: 1), 0, "不同连接仍不能超出同Mac预算")
        var another = RelayFrameBudget()
        XCTAssertEqual(another.reserve(at: 60, frames: 3), 0, "另一Mac的帧预算独立")
    }
    /// 冻结成果跨分片完整重组，最后一片到达前不能展示部分文件。
    /// 数据构造：结果换行共7字节 × 24,000 = 168,000字节，编码数组后超过160KiB单片。
    /// 执行过程：逐片传输 → 首片只记录进度 → 完成后校验原成果/文件字节 → 重复最后一片。
    /// 预期结果：完整正文及任务起点保留，未完成时无成果对象，重复片被拒绝。
    func testFrozenReviewReassemblesBeforeExposingFiles() throws {
        let (original, payload) = try fixture()
        var transfer = original
        let first = chunk(transfer.request, payload: payload)
        XCTAssertNil(try transfer.append(first), "首片不能展示截断成果")
        XCTAssertGreaterThan(transfer.request.offset, 0, "必须推进到下一片原始字节偏移")
        var result: ProjectReviewResponse?
        var last = first
        while result == nil {
            last = chunk(transfer.request, payload: payload)
            result = try transfer.append(last)
        }
        XCTAssertEqual(result?.review.files.first?.after?.text, String(repeating: "结果\n", count: 24_000), "跨片UTF-8不能丢失或替换字符")
        XCTAssertEqual(result?.review.files.first?.before?.text, "before\n", "任务起点仍须可查看")
        XCTAssertEqual(result?.review.files.count, 1, "不得添加原任务之外的文件")
        XCTAssertThrowsError(try transfer.append(last), "已完成传输不能再次接受最后一片")
    }

    /// 错作用域、指纹、偏移、大小与内容身份不能进入成果页面。
    /// 数据构造：同一168KB正文；32MiB=32×1024×1024为总传输上限，160KiB为片上限。
    /// 执行过程：分别篡改封套 → 篡改冻结成果元数据 → 完整传输后解码核验。
    /// 预期结果：每个错误都失败且清除部分内容，不能继续拼接成另一份成果。
    func testMismatchedAndOversizedChunksCannotBecomeAReview() throws {
        let (original, payload) = try fixture()
        var wrongThread = original.request; wrongThread.threadID = UUID().uuidString
        var wrongFingerprint = original.request; wrongFingerprint.fingerprint = String(repeating: "f", count: 64)
        var wrongOffset = original.request; wrongOffset.offset = 1
        var transfer = original
        XCTAssertThrowsError(try transfer.append(chunk(wrongThread, payload: payload)), "另一线程的片不能读取")
        XCTAssertThrowsError(try transfer.append(chunk(original.request, payload: payload)), "失败传输必须重建，不能继续拼接")
        transfer = original
        XCTAssertThrowsError(try transfer.append(chunk(wrongFingerprint, payload: payload)), "另一成果指纹不能读取")
        transfer = original
        XCTAssertThrowsError(try transfer.append(chunk(wrongOffset, payload: payload)), "不能跳过首字节")
        transfer = original
        XCTAssertThrowsError(try transfer.append(.init(request: original.request, totalBytes: 32 * 1024 * 1024 + 1, data: "eA==")), "总长超出32MiB必须立即拒绝")
        transfer = original
        XCTAssertThrowsError(try transfer.append(.init(request: original.request, totalBytes: payload.count, data: Data(repeating: 1, count: 160 * 1024 + 1).base64EncodedString())), "片超出160KiB必须拒绝")
        transfer = original
        XCTAssertThrowsError(try transfer.append(.init(request: original.request, totalBytes: payload.count, data: "not base64")), "无效编码不能变为空文件")
        var body = try XCTUnwrap(JSONSerialization.jsonObject(with: payload) as? [String: Any])
        var delivery = try XCTUnwrap(body["delivery"] as? [String: Any])
        delivery["fingerprint"] = String(repeating: "f", count: 64); body["delivery"] = delivery
        let altered = try JSONSerialization.data(withJSONObject: body)
        transfer = original
        XCTAssertThrowsError(try consume(&transfer, payload: altered), "正确封套不能掩盖正文中另一成果身份")
    }

    private func consume(_ transfer: inout ThreadReviewTransfer, payload: Data) throws -> ProjectReviewResponse {
        while true { if let result = try transfer.append(chunk(transfer.request, payload: payload)) { return result } }
    }
    private func chunk(_ request: ThreadReviewRequest, payload: Data) -> ThreadReviewChunk {
        let end = min(request.offset + 160 * 1024, payload.count)
        return .init(request: request, totalBytes: payload.count, data: payload.subdata(in: request.offset..<end).base64EncodedString())
    }
    private func fixture() throws -> (ThreadReviewTransfer, Data) {
        let outcome: [String: Any] = ["key": ["node_id": 2, "profile": "Codex", "run_id": "run-a", "session_id": "session-a", "turn_id": "turn-a"], "state": "completed", "summary": "原成果"]
        let reference: [String: Any] = ["outcome": outcome, "fingerprint": String(repeating: "a", count: 64), "saved_at_unix": 1]
        var summary = reference
        summary.merge(["sequence": 1, "workspace_id": "project-a", "status": "available", "file_count": 1]) { _, new in new }
        let delivery = try JSONDecoder().decode(TaskDeliverySummary.self, from: JSONSerialization.data(withJSONObject: summary))
        let scope = ProjectReviewRequest(workspaceID: "project-a", routeID: "original-route", deliveryKey: delivery.outcome.key)
        let request = ThreadReviewRequest(collaborationWorkspaceID: UUID().uuidString, threadID: UUID().uuidString,
            operationID: UUID().uuidString, taskRef: "runtime-a:7", fingerprint: delivery.fingerprint, offset: 0)
        let body: [String: Any] = ["request": try JSONSerialization.jsonObject(with: JSONEncoder().encode(scope)), "delivery": reference,
            "review": ["conflicted": false, "files": [["path": "result.txt", "status": " M",
                "before": ["kind": "text", "executable": false, "bytes": Array("before\n".utf8)],
                "after": ["kind": "text", "executable": false, "bytes": Array(String(repeating: "结果\n", count: 24_000).utf8)], "diff": "-before\n+结果\n"]]]]
        return (try ThreadReviewTransfer(request: request, scope: scope, delivery: delivery), try JSONSerialization.data(withJSONObject: body))
    }
}

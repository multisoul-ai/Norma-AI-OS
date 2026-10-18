import XCTest
@testable import E2EKit

final class CaptureRecoveryTests: XCTestCase {
    /// 场景：目标窗口的一次系统截图失败，下一次恢复。
    /// 数据构造：捕获闭包首轮返回 nil，次轮返回文字标记；记录调用次数。
    /// 执行过程：调用窗口捕获重试入口。
    /// 预期结果：返回次轮结果且恰好执行两次，不能把偶发失败立即归为 ENV。
    func testTransientCaptureFailureRetriesAndReturnsNextFrame() async throws {
        var attempts = 0
        let result = try await retryCapture {
            attempts += 1
            return attempts == 2 ? "frame" : nil
        }
        XCTAssertEqual(result, "frame", "第二次捕获应恢复并返回真实帧")
        XCTAssertEqual(attempts, 2, "恢复后不能继续截图")
    }

    /// 场景：macOS Vision 的精确 OCR 模型偶发无法创建计算操作。
    /// 数据构造：精确模式抛出模型错误，快速模式返回一条可用文字。
    /// 执行过程：通过截图 OCR 恢复入口执行两种模式。
    /// 预期结果：返回快速模式结果且恰好各尝试一次，不把识别器故障当产品画面失败。
    func testVisionFailureFallsBackToFastRecognition() throws {
        var accurateCalls = 0
        var fastCalls = 0
        let text = try recoverVisionRecognition(
            accurate: { () throws -> String in
                accurateCalls += 1
                throw NSError(domain: "Vision", code: 13)
            },
            fast: {
                fastCalls += 1
                return "创建工作空间"
            }
        )
        XCTAssertEqual(text, "创建工作空间", "快速识别应恢复真实文字")
        XCTAssertEqual(accurateCalls, 1, "精确识别失败后不能反复调用")
        XCTAssertEqual(fastCalls, 1, "快速识别应只尝试一次")
    }

    /// 场景：精确和快速 Vision 识别均失败，截图文字无法用于产品断言。
    /// 数据构造：两种识别闭包都抛出系统模型错误。
    /// 执行过程：运行 OCR 恢复入口并检查分类。
    /// 预期结果：返回 ENV 错误，不能伪装成产品画面不含目标文字。
    func testVisionDoubleFailureReportsEnvironment() {
        XCTAssertThrowsError(
            try recoverVisionRecognition(
                accurate: { () throws -> String in throw NSError(domain: "Vision", code: 13) },
                fast: { () throws -> String in throw NSError(domain: "Vision", code: 13) }
            )
        ) { error in
            guard let failure = error as? E2EError else {
                return XCTFail("双模式 OCR 失败应分类为 ENV，实际：\(error)")
            }
            XCTAssertEqual(failure.kind, .env, "双模式 OCR 失败不能误判为产品问题")
            XCTAssertTrue(failure.message.contains("Vision"), "ENV 原因应指出 Vision 识别器")
        }
    }

    /// 场景：工作树路径很深，Unix socket 不能使用证据目录的完整路径。
    /// 数据构造：传入 240 字符的运行标识。
    /// 执行过程：生成隔离 socket 路径。
    /// 预期结果：路径位于 /tmp，低于 macOS 的 104 字节界限，且不落到正式版 socket。
    func testSocketPathStaysShortForDeepWorktree() {
        let path = isolatedSocketPath(identifier: String(repeating: "a", count: 240))
        XCTAssertTrue(path.hasPrefix("/tmp/norma-e2e-"), "隔离 socket 应位于短临时路径")
        XCTAssertLessThan(path.utf8.count, 104, "Unix socket 路径必须低于 SUN_LEN")
        XCTAssertNotEqual(path, "/tmp/multisoul-norma.sock", "不能占用正式版 socket")
    }
}

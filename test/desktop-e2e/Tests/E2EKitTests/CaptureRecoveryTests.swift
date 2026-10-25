import XCTest
@testable import E2EKit

final class CaptureRecoveryTests: XCTestCase {
    /// 场景：同进程的两种精确识别都因执行上下文故障失败，独立进程仍能读出当前截图。
    /// 构造：默认与 CPU 各抛不同原错，独立进程返回按钮，快速只返回标题。
    /// 执行：生产恢复策略逐级尝试，记录故障及恢复轨迹。
    /// 预期：采用独立进程的精确结果，原错误均保留，不调用快速识别。
    func testProcessIsolationRecoversWithoutLosingInProcessFailures() throws {
        var events: [String] = []
        var fastCalls = 0
        let result = try recoverVisionRecognition(
            accurate: { () throws -> String in throw NSError(domain: "e5rt", code: 13) },
            cpuAccurate: { throw NSError(domain: "CPUContext", code: 13) },
            isolatedAccurate: { "Create Workspace" },
            record: { stage, detail in events.append("\(stage): \(detail)") },
            fast: { fastCalls += 1; return "Norma" }
        )
        XCTAssertEqual(result, "Create Workspace", "独立进程必须返回精确识别的按钮")
        XCTAssertEqual(fastCalls, 0, "精确恢复后不能使用快速漏字结果")
        for expected in ["e5rt", "CPUContext", "isolated-accurate-recovered"] {
            XCTAssertTrue(events.contains { $0.contains(expected) }, "恢复证据不能丢失 \(expected)")
        }
    }

    /// 场景：默认精确识别正常，不应调用任何降级路径。
    /// 构造：默认返回按钮，CPU 与快速调用均计数。
    /// 执行：使用生产识别恢复入口。
    /// 预期：直接返回精确文字，降级调用数为零。
    func testAccurateSuccessDoesNotInvokeFallbacks() throws {
        var fallbacks = 0
        let result = try recoverVisionRecognition(accurate: { "Create Workspace" },
            cpuAccurate: { fallbacks += 1; return "CPU" },
            fast: { fallbacks += 1; return "Fast" })
        XCTAssertEqual(result, "Create Workspace", "默认成功必须直接返回")
        XCTAssertEqual(fallbacks, 0, "不能额外调用降级识别器")
    }

    /// 场景：默认和 CPU 精确模式都执行失败，快速模式非空但漏字。
    /// 构造：两条不同原始错误与只有标题的快速结果。
    /// 执行：通过生产恢复入口走完整降级链。
    /// 预期：ENV/driver，保留两个原因及快速诊断，不能进入产品断言。
    func testBothAccurateFailuresRetainEveryCause() {
        XCTAssertThrowsError(try recoverVisionRecognition(
            accurate: { () throws -> String in throw NSError(domain: "e5rt", code: 13) },
            cpuAccurate: { throw NSError(domain: "CPUDevice", code: 27) },
            isolatedAccurate: { throw E2EError.driver("worker status=3") }, fast: { "Norma" }
        )) { error in
            guard let failure = error as? E2EError else { return XCTFail("应返回驱动错误：\(error)") }
            XCTAssertEqual(failure.kind, .env, "不完整文字不能用作产品证据")
            XCTAssertEqual(failure.layer, "driver", "应明确 OCR 驱动层")
            for cause in ["e5rt", "13", "CPUDevice", "27", "worker status=3", "Norma"] {
                XCTAssertTrue(failure.message.contains(cause), "必须保留原始诊断 \(cause)")
            }
        }
    }

    /// 场景：精确 OCR 执行失败，快速 OCR 返回了标题但漏掉仍可见的创建按钮。
    /// 数据构造：注入 Vision code 13 和只有 Norma 的快速结果，模拟已归档失败画面。
    /// 执行过程：调用实际截图所用的识别恢复入口。
    /// 预期结果：不把不完整的快速结果当成可靠画面；保留原始故障和快速结果，分类 ENV。
    func testIncompleteFastRecognitionCannotBecomeProductEvidence() {
        XCTAssertThrowsError(try recoverVisionRecognition(
            accurate: { () throws -> [String] in
                throw NSError(domain: "e5rt", code: 13)
            },
            fast: { ["Norma"] }
        )) { error in
            guard let failure = error as? E2EError else {
                return XCTFail("识别降级失败必须保留 ENV 分类：\(error)")
            }
            XCTAssertEqual(failure.kind, .env, "按钮漏识别不能作为产品故障证据")
            XCTAssertTrue(failure.message.contains("e5rt"), "必须保留原始模型执行错误")
            XCTAssertTrue(failure.message.contains("13"), "必须保留原始错误码")
            XCTAssertTrue(failure.message.contains("Norma"), "必须保留快速模式的诊断结果")
        }
    }

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

    /// 场景：默认精确 OCR 执行失败，快速模式会漏掉按钮，CPU 精确模式可恢复。
    /// 数据构造：默认抛出 code 13，CPU 返回创建按钮，快速仅返回标题。
    /// 执行过程：通过实际截图使用的恢复入口取得文字，并收集原始故障。
    /// 预期结果：采用可靠的 CPU 结果，保留 code 13，不接受漏字结果。
    func testVisionFailureRecoversAccurateTextOnCPUAndRetainsCause() throws {
        var accurateCalls = 0
        var fastCalls = 0
        var causes: [String] = []
        let text = try recoverVisionRecognition(
            accurate: { () throws -> String in
                accurateCalls += 1
                throw NSError(domain: "Vision", code: 13)
            },
            cpuAccurate: { "创建工作空间" },
            record: { mode, detail in causes.append("\(mode): \(detail)") },
            fast: {
                fastCalls += 1
                return "Norma"
            }
        )
        XCTAssertEqual(text, "创建工作空间", "CPU 精确识别应恢复实际按钮文字")
        XCTAssertEqual(accurateCalls, 1, "精确识别失败后不能反复调用")
        XCTAssertEqual(fastCalls, 0, "可靠识别已恢复，不能再用快速模式降低断言质量")
        XCTAssertTrue(causes.contains { $0.contains("Vision") && $0.contains("13") }, "恢复成功也必须留下原始错误")
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

import CoreGraphics
import XCTest
@testable import E2EKit

final class VisualReviewTests: XCTestCase {
    private func request(kind: String = "check") -> VisualRequest {
        VisualRequest(id: "frame-1", screenshot: "frame.png", sha256: "abc", kind: kind,
            width: 1440, height: 840, pixelWidth: 2880, pixelHeight: 1680,
            criteria: [VisualCriterion(id: "cards", text: "八张卡完整"), VisualCriterion(id: "gaps", text: "无隐藏空档")])
    }
    private func reply() -> VisualResponse {
        VisualResponse(requestID: "frame-1", screenshotSHA256: "abc", reviewer: "current-agent",
            findings: [VisualFinding(id: "cards", verdict: "pass", observation: "六Agent两终端可读"),
                       VisualFinding(id: "gaps", verdict: "pass", observation: "首格和各列无空档")], point: nil)
    }

    /// 场景：逐项看图。构造：两项完整结论及缺项、重复、空理由。
    /// 执行：校验响应。预期：仅完整判断通过，截图存在不能替代结论。
    func testRequiresEveryCriterionExactlyOnceWithObservation() throws {
        XCTAssertTrue(try validateVisualResponse(reply(), for: request()), "完整结论通过")
        var response = reply(); response.findings = []
        XCTAssertThrowsError(try validateVisualResponse(response, for: request()), "缺项不得通过")
        response = reply(); response.findings[1] = response.findings[0]
        XCTAssertThrowsError(try validateVisualResponse(response, for: request()), "重复不得补齐缺项")
        response = reply(); response.findings[0] = VisualFinding(id: "cards", verdict: "pass", observation: " ")
        XCTAssertThrowsError(try validateVisualResponse(response, for: request()), "空理由不得通过")
    }

    /// 场景：响应来自旧图。构造：分别改变 ID 与 SHA256。
    /// 执行：与当前请求比较。预期：两种错配均拒绝。
    func testRejectsResponseForDifferentFrame() {
        var response = reply(); response.requestID = "old"
        XCTAssertThrowsError(try validateVisualResponse(response, for: request()), "旧 ID 无效")
        response = reply(); response.screenshotSHA256 = "different"
        XCTAssertThrowsError(try validateVisualResponse(response, for: request()), "图片指纹必须一致")
    }

    /// 场景：明确失败或看不清。构造：fail/uncertain 与 wait 同时出现。
    /// 执行：校验响应。预期：PRODUCT/ENV 分开且立即失败，不能被 wait 掩盖。
    func testFailureAndUncertaintyRemainDistinct() {
        for (verdict, kind) in [("fail", E2EError.Kind.product), ("uncertain", .env)] {
            var response = reply(); response.findings[0].verdict = verdict; response.findings[1].verdict = "wait"
            XCTAssertThrowsError(try validateVisualResponse(response, for: request())) { error in
                XCTAssertEqual((error as? E2EError)?.kind, kind, "保留错误类别")
            }
        }
    }

    /// 场景：Retina 图定位。构造：2×图、窗口点坐标、缺失及越界点。
    /// 执行：校验定位。预期：仅窗口内部点合法，wait 不得附带点击坐标。
    func testLocationRequiresInBoundsWindowPoint() throws {
        var response = reply()
        XCTAssertThrowsError(try validateVisualResponse(response, for: request(kind: "locate")), "缺坐标拒绝")
        response.point = CGPoint(x: 1200, y: 800)
        XCTAssertTrue(try validateVisualResponse(response, for: request(kind: "locate")), "窗口点不乘 Retina 倍数")
        for point in [CGPoint(x: -1, y: 1), CGPoint(x: 1440, y: 1), CGPoint(x: 1, y: 840)] {
            response.point = point
            XCTAssertThrowsError(try validateVisualResponse(response, for: request(kind: "locate")), "越界不发送")
        }
        response.point = CGPoint(x: 10, y: 10); response.findings[0].verdict = "wait"
        XCTAssertThrowsError(try validateVisualResponse(response, for: request(kind: "locate")), "未就绪不点击")
    }

    /// 场景：思考期间窗口变了。构造：原窗口、高度改变、ID改变。
    /// 执行：输入前校验。预期：只有同窗口同尺寸有效。
    func testChangedWindowInvalidatesLocation() {
        XCTAssertNoThrow(try validateVisualWindow(id: 7, size: CGSize(width: 1440, height: 840), expectedID: 7, request: request()))
        XCTAssertThrowsError(try validateVisualWindow(id: 7, size: CGSize(width: 1440, height: 700), expectedID: 7, request: request()), "高度改变需新截图")
        XCTAssertThrowsError(try validateVisualWindow(id: 8, size: CGSize(width: 1440, height: 840), expectedID: 7, request: request()), "旧窗口坐标拒绝")
    }

    /// 场景：没有响应。构造：不存在的文件、零预算。
    /// 执行：等待模型。预期：driver ENV，不能默认通过。
    func testMissingResponseTimesOut() async {
        do {
            _ = try await awaitVisualResponse(at: URL(fileURLWithPath: "/tmp/missing-\(UUID()).json"), timeout: 0, ensureRunning: {})
            XCTFail("缺响应不能继续")
        } catch { XCTAssertEqual((error as? E2EError)?.layer, "driver", "明确驱动超时") }
    }

    /// 场景：加载后成功。构造：第一帧 wait、第二帧 pass。
    /// 执行：只重做观察。预期：新帧才通过，wait 不算检查完成。
    func testLoadingRequestsAnotherObservation() async throws {
        var captures = 0
        let result = try await retryVisualReview(intervalNanoseconds: 0) { () async throws -> String? in
            captures += 1
            var response = self.reply()
            if captures == 1 { response.findings[0].verdict = "wait" }
            return try validateVisualResponse(response, for: self.request()) ? "new-frame" : nil
        }
        XCTAssertEqual(result, "new-frame", "加载帧不能完成检查")
        XCTAssertEqual(captures, 2, "只重新观察一次")
    }

    /// 场景：持续加载与明确失败。构造：持续nil，以及首次抛产品错误。
    /// 执行：有限观察。预期：最多三次，产品失败立即传播。
    func testObservationBudgetAndFailure() async {
        var captures = 0
        do {
            let _: String = try await retryVisualReview(intervalNanoseconds: 0) { captures += 1; return nil }
            XCTFail("持续加载不得通过")
        } catch { XCTAssertEqual((error as? E2EError)?.layer, "driver", "耗尽是驱动阻塞") }
        XCTAssertEqual(captures, 3, "三次预算")
        captures = 0
        do {
            let _: String = try await retryVisualReview(intervalNanoseconds: 0) { captures += 1; throw E2EError.product("细条") }
            XCTFail("明确失败不能重试到绿")
        } catch { XCTAssertEqual((error as? E2EError)?.kind, .product, "保留产品错误") }
        XCTAssertEqual(captures, 1, "只有明确未就绪才重试")
    }

    /// 场景：模型模式误入OCR或未检查就结束。构造：未启动的模型Session。
    /// 执行：capture与finishSuccess。预期：均拒绝，不调用Vision或输出假通过。
    @MainActor
    func testModelModeRejectsImplicitOCRAndEmptySuccess() async {
        let session = Session(scenario: "test", appPath: "/missing", artifactRoot: "/tmp", visualReview: true)
        do { _ = try await session.capture(); XCTFail("禁止OCR") }
        catch { XCTAssertTrue(String(describing: error).contains("OCR"), "先拦截OCR入口") }
        XCTAssertThrowsError(try session.finishSuccess(), "无检查不能通过")
    }
}

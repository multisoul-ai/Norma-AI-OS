import XCTest
import CoreGraphics
@testable import E2EKit

final class ActionRecoveryTests: XCTestCase {
    /// 场景：动作前观察就遇到不可恢复的 OCR 驱动失败。
    /// 构造：peek 抛 ENV，输入和后置等待均计数。
    /// 执行：生产重试入口有四次预算。
    /// 预期：立即保留原错，输入与等待均未执行，并明确记录动作未开始。
    @MainActor func testObservationFailureRecordsActionNotStarted() async {
        var inputs = 0
        var waits = 0
        var stages: [String] = []
        do {
            let _: String = try await retryAction("创建", attempts: 4,
                peek: { throw E2EError.driver("e5rt code 13") },
                action: { inputs += 1 }, wait: { _ in waits += 1; return "unexpected" },
                record: { _, stage, _ in stages.append(stage) })
            XCTFail("观察驱动失败不能进入产品断言")
        } catch let failure as E2EError {
            XCTAssertEqual(failure.message, "e5rt code 13", "必须保留最早的 OCR 原因")
            XCTAssertEqual(failure.layer, "driver", "观察失败属于驱动层")
        } catch { XCTFail("错误分类丢失：\(error)") }
        XCTAssertEqual(inputs, 0, "不得发送输入")
        XCTAssertEqual(waits, 0, "不得等待产品结果")
        XCTAssertTrue(stages.contains("action-not-started"), "报告应明确本次动作未开始")
        XCTAssertFalse(stages.contains("action-completed"), "不能伪报动作完成")
    }

    /// 场景：按钮定位失败，点击未发送，重试不能把原始错误覆盖成对话框等待超时。
    /// 数据构造：每次观察均未满足，定位抛出具体错误；等待闭包会产生不同超时错误。
    /// 执行过程：通过生产 act 使用的重试入口尝试两次，并记录实际发送与等待次数。
    /// 预期结果：最终仍为定位错误，没有鼠标发送，也从未等待未执行动作的后置状态。
    @MainActor func testUnsentClickKeepsOriginalActionFailure() async {
        var actionCalls = 0
        var waits = 0
        var sentClicks: [String] = []
        var events: [String] = []
        do {
            let _: String = try await retryAction("打开创建", attempts: 2, peek: { nil }, action: {
                actionCalls += 1
                try await deliverClick(resolve: {
                    throw E2EError.product("未识别到 Create Workspace")
                }, send: { phase, _ in sentClicks.append(phase) }, pause: { _ in })
            }, wait: { _ in
                waits += 1
                throw E2EError.product("等待对话框超时")
            }, record: { _, stage, detail in events.append("\(stage): \(detail)") })
            XCTFail("没有发送点击时不能宣布动作成功")
        } catch let failure as E2EError {
            XCTAssertEqual(failure.message, "未识别到 Create Workspace", "必须保留原始定位错误")
            XCTAssertFalse(failure.message.contains("超时"), "等待错误不能覆盖未执行动作的原因")
        } catch { XCTFail("应保留驱动的结构化错误：\(error)") }
        XCTAssertEqual(actionCalls, 2, "允许按预算重新定位按钮")
        XCTAssertEqual(waits, 0, "点击未发送时不能等待点击结果")
        XCTAssertTrue(sentClicks.isEmpty, "定位失败不能发送盲点点击")
        XCTAssertTrue(events.contains { $0.contains("未识别到 Create Workspace") }, "每次动作故障应留证据")
    }

    /// 场景：可靠识别后已经发送输入，但产品仍没有显示后置状态。
    /// 数据构造：输入闭包成功记录 mouse-up，等待闭包始终返回产品超时。
    /// 执行过程：实际重试入口执行一次输入和后置断言。
    /// 预期结果：保留 PRODUCT 超时，不把已执行动作的产品失败重标为 ENV。
    @MainActor func testCompletedInputStillRequiresProductPostcondition() async {
        var sent: [String] = []
        do {
            let _: String = try await retryAction("创建", attempts: 1, peek: { nil }, action: {
                sent.append("mouse-up acknowledged")
            }, wait: { _ in throw E2EError.product("创建后数据库仍为空") })
            XCTFail("输入完成并不等于产品后置条件满足")
        } catch let failure as E2EError {
            XCTAssertEqual(failure.kind, .product, "真实后置条件失败必须保留 PRODUCT")
            XCTAssertEqual(failure.message, "创建后数据库仍为空", "必须保留产品断言原因")
        } catch { XCTFail("必须返回产品断言错误：\(error)") }
        XCTAssertEqual(sent, ["mouse-up acknowledged"], "本场景必须确实经过输入路径")
    }
}

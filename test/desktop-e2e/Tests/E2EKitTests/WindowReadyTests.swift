import XCTest
@testable import E2EKit

final class WindowReadyTests: XCTestCase {
    /// 场景：socket 先开放，窗口随后创建。
    /// 构造：依次返回未连接、no window、真实窗口响应。
    /// 执行：调用 resize 使用的窗口就绪等待，零间隔、三次预算。
    /// 预期：只读探测成功后返回原窗口数据，不提前把启动时序判为产品失败。
    func testWaitsForWindowAfterSocketOpens() async throws {
        var calls = 0
        let result = try await waitForControlWindow(attempts: 3, intervalNanoseconds: 0) {
            calls += 1
            switch calls {
            case 1: return nil
            case 2: return ["ok": false, "error": "no window"]
            default: return ["ok": true, "window": ["number": 7, "width": 1440, "height": 840]]
            }
        }
        XCTAssertEqual(calls, 3, "socket 开放不等于窗口就绪")
        XCTAssertEqual((result["window"] as? [String: Int])?["number"], 7, "保留真正就绪的窗口响应")
    }

    /// 场景：鉴权等其他拒绝不得当成窗口启动慢。
    /// 构造：第一次返回 unauthorized，后续本可成功。
    /// 执行：给三次等待预算。
    /// 预期：立即保留原始拒绝，只探测一次。
    func testOtherRejectionsAreNotRetried() async {
        var calls = 0
        do {
            _ = try await waitForControlWindow(attempts: 3, intervalNanoseconds: 0) {
                calls += 1
                return ["ok": false, "error": "unauthorized"]
            }
            XCTFail("鉴权失败不得被窗口等待吞掉")
        } catch let error as E2EError {
            XCTAssertTrue(error.message.contains("unauthorized"), "保留原始拒绝原因")
        } catch { XCTFail("应返回结构化错误：\(error)") }
        XCTAssertEqual(calls, 1, "只允许重试明确的 no window 或尚未连接")
    }

    /// 场景：进程始终没有窗口，等待必须有界。
    /// 构造：两次都返回 no window。
    /// 执行：使用两次预算。
    /// 预期：两次后失败且明确窗口未就绪，不能返回伪成功。
    func testMissingWindowHasBoundedFailure() async {
        var calls = 0
        do {
            _ = try await waitForControlWindow(attempts: 2, intervalNanoseconds: 0) {
                calls += 1
                return ["ok": false, "error": "no window"]
            }
            XCTFail("没有窗口不能报告就绪")
        } catch let error as E2EError {
            XCTAssertTrue(error.message.contains("窗口未就绪"), "超时应明确窗口前置条件失败")
        } catch { XCTFail("应返回结构化错误：\(error)") }
        XCTAssertEqual(calls, 2, "等待不得超过给定预算")
    }

    /// 场景：等待期间 App 已死，不能继续空等。
    /// 构造：只读探测抛出生产死亡错误。
    /// 执行：使用三次预算。
    /// 预期：原错原样传播，不重试。
    func testProcessDeathPropagatesImmediately() async {
        var calls = 0
        do {
            _ = try await waitForControlWindow(attempts: 3, intervalNanoseconds: 0) {
                calls += 1
                throw E2EError.product("隔离 Norma 进程已退出")
            }
            XCTFail("已死进程不能报告窗口就绪")
        } catch let error as E2EError {
            XCTAssertEqual(error.message, "隔离 Norma 进程已退出", "保留原始进程死亡原因")
        } catch { XCTFail("应返回结构化错误：\(error)") }
        XCTAssertEqual(calls, 1, "进程死亡后不再探测")
    }
}

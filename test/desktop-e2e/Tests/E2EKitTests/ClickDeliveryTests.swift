import CoreGraphics
import Foundation
import Testing
@testable import E2EKit

@MainActor
struct ClickDeliveryTests {
    /// 场景：控制通道明确报告被测产品崩溃，而非响应丢失。
    /// 构造：定位成功，传输抛出已有 PRODUCT 错误。
    /// 执行：经过生产点击发送边界。
    /// 预期：保留产品崩溃分类及原文，不把所有输入阶段异常强行归入驱动。
    @Test func productCrashDuringInputKeepsProductClassification() async {
        do {
            try await deliverClick(resolve: { CGPoint(x: 12, y: 34) },
                send: { _, _ in throw E2EError.product("App exited status=11") }, pause: { _ in })
            Issue.record("产品崩溃不能成功")
        } catch let failure as E2EError {
            #expect(failure.kind == .product, "已确认的产品失败不能改成 ENV")
            #expect(failure.message == "App exited status=11", "保留原始崩溃原因")
        } catch { Issue.record("错误分类丢失：\(error)") }
    }

    /// 场景：OCR 定位失败发生在任何鼠标输入之前。
    /// 构造：定位抛出 Vision 原因，传输闭包记录所有实际调用。
    /// 执行：调用与生产点击共用的发送入口。
    /// 预期：原错完整传播，留下明确未发送证据，没有发送尝试或确认。
    @Test func unresolvedTargetNeverSendsInput() async {
        var sent: [String] = []
        var events: [String] = []
        do {
            try await deliverClick(resolve: { throw E2EError.driver("e5rt code 13") },
                send: { phase, _ in sent.append(phase) }, pause: { _ in },
                record: { stage, detail in events.append("\(stage): \(detail)") })
            Issue.record("定位失败不能成功")
        } catch let failure as E2EError {
            #expect(failure.message == "e5rt code 13", "原始识别错误必须保留")
        } catch { Issue.record("错误类型丢失：\(error)") }
        #expect(sent.isEmpty, "不能盲点点击")
        #expect(events.contains { $0.contains("not-sent") && $0.contains("e5rt code 13") }, "必须证明失败发生在发送之前")
        #expect(!events.contains { $0.contains("dispatch-attempted") || $0.contains("acknowledged") }, "未发送不能伪造发送或确认")
    }

    /// 场景：可靠定位后完整派发点击。
    /// 构造：夹具坐标代表 OCR 输出，外部传输逐次成功应答。
    /// 执行：实际入口发送 move/down/up，收集传输及证据序列。
    /// 预期：每个阶段先尝试后确认，最终才标记完成。
    @Test func successfulClickRecordsEachAcknowledgement() async throws {
        var sent: [String] = []
        var events: [String] = []
        try await deliverClick(resolve: { CGPoint(x: 12, y: 34) },
            send: { phase, _ in sent.append(phase) }, pause: { _ in },
            record: { stage, detail in events.append("\(stage): \(detail)") })
        #expect(sent == ["move", "down", "up"], "必须真实执行全部输入阶段")
        for phase in sent {
            let attempted = events.firstIndex { $0.contains("dispatch-attempted") && $0.contains("phase=\(phase)") }
            let acknowledged = events.firstIndex { $0.contains("acknowledged") && $0.contains("phase=\(phase)") }
            #expect(attempted != nil && acknowledged != nil, "每个输入阶段都必须有尝试和 ACK")
            if let attempted, let acknowledged {
                #expect(attempted < acknowledged, "控制通道应答前不能标记已确认")
            }
        }
        #expect(events.last?.contains("completed") == true, "全部应答后才确认点击完成")
        #expect(!events.contains { $0.contains("not-sent") || $0.contains("delivery-unknown") }, "成功路径不应混入失败状态")
    }

    /// 场景：mouse-down 请求之后控制通道响应丢失，不能判断远端是否已接收。
    /// 构造：move 成功，down 抛超时；记录发送阶段及报告。
    /// 执行：经过实际输入入口，捕获错误并用生产报告写入 JSON。
    /// 预期：ENV/driver，保留原始原因；down 无 ACK，up 未发送，报告保留全部证据。
    @Test func lostResponseIsUnknownDeliveryAndReportRetainsCause() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let session = Session(scenario: "input-regression", appPath: "/unused", artifactRoot: root.path)
        var sent: [String] = []
        do {
            try await deliverClick(resolve: { CGPoint(x: 12, y: 34) }, send: { phase, _ in
                sent.append(phase)
                if phase == "down" { throw NSError(domain: "ControlResponseLost", code: 408) }
            }, pause: { _ in }, record: { session.recordDriverEvent("click", $0, $1) })
            Issue.record("响应丢失不能确认成功")
        } catch {
            session.finishFailure(error)
        }
        let data = try Data(contentsOf: root.appendingPathComponent("report.json"))
        let report = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        #expect(report["kind"] as? String == "env", "输入驱动故障不能归 PRODUCT")
        #expect(report["failureLayer"] as? String == "driver", "报告必须明确驱动层")
        #expect((report["message"] as? String)?.contains("ControlResponseLost") == true, "原始传输原因不能丢失")
        let events = try #require(report["driverEvents"] as? [[String: Any]])
        #expect(events.contains { $0["stage"] as? String == "delivery-unknown" }, "响应失败只能标记送达未知")
        #expect(!events.contains { $0["stage"] as? String == "acknowledged" && ($0["detail"] as? String)?.contains("phase=down") == true }, "不能伪报 down 已确认")
        #expect(!events.contains { ["completed", "not-sent"].contains($0["stage"] as? String ?? "") }, "不能声称未发送或完整完成")
        #expect(sent == ["move", "down"], "失败后不得继续发 up 或重发点击")
    }
}

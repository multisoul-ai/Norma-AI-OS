import Foundation
import Testing
@testable import E2EKit

struct OCRProcessTests {
    /// 场景：独立精确 OCR 子进程退出失败。
    /// 构造：shell 夹具向 stderr 写具体原因并以 3 退出。
    /// 执行：走生产的有界子进程读取边界。
    /// 预期：保留 stderr 与退出码，ENV/driver，不能返回伪造文本。
    @Test func workerFailureRetainsOriginalStderrAndExitStatus() {
        do {
            _ = try runRecognitionWorker(executable: URL(fileURLWithPath: "/bin/sh"),
                arguments: ["-c", "printf 'original Vision code 13' >&2; exit 3"])
            Issue.record("失败子进程不能返回识别结果")
        } catch let failure as E2EError {
            #expect(failure.kind == .env && failure.layer == "driver", "必须明确驱动失败")
            #expect(failure.message.contains("original Vision code 13"), "stderr 原错不能丢失")
            #expect(failure.message.contains("status=3"), "必须记录真实退出码")
        } catch { Issue.record("错误分类丢失：\(error)") }
    }

    /// 场景：OCR 子进程无响应。
    /// 构造：sleep 10 秒，驱动预算 0.05 秒，远小于夹具运行时长。
    /// 执行：启动实际子进程并触发生产超时收尾。
    /// 预期：错误明确超时，3 秒以内返回，不等待完整 10 秒。
    @Test func workerTimeoutIsBounded() {
        let start = Date()
        do {
            _ = try runRecognitionWorker(executable: URL(fileURLWithPath: "/bin/sleep"), arguments: ["10"], timeout: 0.05)
            Issue.record("超时不能返回识别结果")
        } catch let failure as E2EError {
            #expect(failure.message.contains("超时"), "超时不能伪装为按钮缺失")
        } catch { Issue.record("错误分类丢失：\(error)") }
        #expect(Date().timeIntervalSince(start) < 3, "必须及时终止并回收本次子进程")
    }

    /// 场景：子进程返回成功却没有合法 JSON。
    /// 构造：true 以 0 退出且没有输出。
    /// 执行：生产反序列化边界读取输出。
    /// 预期：拒绝无效结果，不能作为空文字去通过 absent 断言。
    @Test func malformedOutputCannotBecomeEmptyTextEvidence() {
        do {
            _ = try runRecognitionWorker(executable: URL(fileURLWithPath: "/usr/bin/true"), arguments: [])
            Issue.record("没有 JSON 不能当作空画面")
        } catch let failure as E2EError {
            #expect(failure.message.contains("JSON"), "报告应指出协议错误")
            #expect(failure.layer == "driver", "协议失败属于驱动层")
        } catch { Issue.record("错误分类丢失：\(error)") }
    }
}

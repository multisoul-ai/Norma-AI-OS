import XCTest
@testable import NormaCore

final class TaskTimingTests: XCTestCase {
    /// 步骤字段只接受完整有效的有界计划，旧Mac不显示假进度。
    /// 数据构造：1/2为合法；3/2、0总数、65=64上限+1、257=标题256上限+1为非法。
    /// 执行过程：解码旧卡 → 加入合法计划 → 依次验证越界和无效标题。
    /// 预期结果：旧卡nil、合法阶段保留；每个坏值被拒绝，不能污染页面或扩大分段数量。
    func testOptionalProgressValidation() throws {
        let legacy = Data(#"{"node_id":1,"profile":"Claude","title":"A","state":"thinking","mobile_control":true}"#.utf8)
        var card = try JSONDecoder().decode(CardSnapshot.self, from: legacy)
        XCTAssertNil(card.taskProgress, "没有计划时不能制造数字")
        card.taskProgress = try JSONDecoder().decode(TaskProgress.self, from: Data(#"{"completed":1,"total":2,"current_step":"Checking beta"}"#.utf8))
        XCTAssertNoThrow(try card.validate(), "确认的完整计划应可展示")
        XCTAssertEqual(card.taskProgress?.currentStep, "Checking beta", "保留Provider的真实阶段")
        card.taskProgress = TaskProgress(completed: 3, total: 2, currentStep: nil)
        XCTAssertThrowsError(try card.validate(), "完成数不能超过总数")
        card.taskProgress = TaskProgress(completed: 0, total: 0, currentStep: nil)
        XCTAssertThrowsError(try card.validate(), "未知不能用0/0表示")
        card.taskProgress = TaskProgress(completed: 0, total: 65, currentStep: nil)
        XCTAssertThrowsError(try card.validate(), "不能生成超过64段的布局")
        card.taskProgress = TaskProgress(completed: 0, total: 1, currentStep: String(repeating: "A", count: 257))
        XCTAssertThrowsError(try card.validate(), "阶段标题上限为256字符")
    }
    /// 耗时来自可选服务器字段，运行时本地推进，完成时固定，不把旧服务当作零耗时。
    ///
    /// 数据构造：Unix起点100000ms=100s；当前220s→经过120s=2分钟；完成耗时90000ms=90s。
    /// 执行过程：解码旧快照 → 解码运行计时 → 本地前进60s → 完成 → 手机时钟早于起点。
    /// 预期结果：旧服务不伪造耗时；完成不会继续增长；负时间被钳为0，不产生负分钟。
    func testOptionalTimingAndLocalElapsedWithoutNetworkUpdates() throws {
        let legacy = Data(#"{"node_id":1,"profile":"Codex","title":"A","state":"thinking","mobile_control":true}"#.utf8)
        let card = try JSONDecoder().decode(CardSnapshot.self, from: legacy)
        XCTAssertNil(card.taskTiming, "旧Mac缺字段时不伪造耗时")
        let running = try JSONDecoder().decode(TaskTiming.self, from: Data(#"{"started_at_unix_ms":100000}"#.utf8))
        XCTAssertEqual(running.elapsedMinutes(at: Date(timeIntervalSince1970: 220)), 2, "运行时按已知起点本地显示")
        XCTAssertEqual(running.elapsedMinutes(at: Date(timeIntervalSince1970: 280)), 3, "一分钟后不依赖新网络快照")
        let done = try JSONDecoder().decode(TaskTiming.self, from: Data(#"{"started_at_unix_ms":100000,"completed_elapsed_ms":90000}"#.utf8))
        XCTAssertEqual(done.elapsedMinutes(at: Date(timeIntervalSince1970: 1000)), 1, "完成用Mac单调时钟90s，不能增长到15分钟")
        XCTAssertNotEqual(done.elapsedMinutes(at: Date(timeIntervalSince1970: 1000)), 15, "不能按手机现在时间重新计算已完成耗时")
        XCTAssertEqual(running.elapsedMinutes(at: Date(timeIntervalSince1970: 50)), 0, "手机时钟偏早不能显示负耗时")
    }
}

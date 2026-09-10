import XCTest
@testable import NormaCore

final class TaskSummaryTests: XCTestCase {
    /// 新快照携带有界任务摘录，旧电脑缺字段仍可读，超长和控制字符不能进入显示。
    /// 数据构造：一张Codex卡；240为双方约定的Unicode scalar上限，241=240+1为越界；旧快照不含该键。
    /// 执行过程：解码正常/旧快照 → 校验 → 重编码；分别校验超长和控制字符输入。
    /// 预期结果：正常摘录保留、旧快照不伪造文字、越界及控制字符明确失败。
    func testTaskSummaryBoundsAndCompatibility() throws {
        var raw: [String: Any] = ["node_id": 1, "profile": "Codex", "title": "连接", "mobile_control": true,
                                  "state": "done", "task_summary": "已修复连接并完成测试"]
        func card(_ raw: [String: Any]) throws -> CardSnapshot {
            try JSONDecoder().decode(CardSnapshot.self, from: JSONSerialization.data(withJSONObject: raw))
        }
        let current = try card(raw)
        try current.validate()
        let encoded = try JSONSerialization.jsonObject(with: JSONEncoder().encode(current)) as? [String: Any]
        XCTAssertEqual(encoded?["task_summary"] as? String, "已修复连接并完成测试", "新电脑的真实摘录不能被解码丢弃")
        raw.removeValue(forKey: "task_summary")
        let old = try card(raw); try old.validate()
        let oldEncoded = try JSONSerialization.jsonObject(with: JSONEncoder().encode(old)) as? [String: Any]
        XCTAssertNil(oldEncoded?["task_summary"], "旧电脑不能伪造任务摘要")
        raw["task_summary"] = String(repeating: "界", count: 241)
        XCTAssertThrowsError(try card(raw).validate(), "超出240标量的输入必须失败")
        raw["task_summary"] = "坏\u{001B}输入"
        XCTAssertThrowsError(try card(raw).validate(), "终端控制字符不能进入卡片摘要")
        raw["task_summary"] = String(repeating: "界", count: 240)
        XCTAssertNoThrow(try card(raw).validate(), "恰好240标量必须可读")
        raw["task_summary"] = "已完成 👩‍💻 测试"
        XCTAssertNoThrow(try card(raw).validate(), "组合emoji中的连接符不能误判成终端控制字符")
    }
}

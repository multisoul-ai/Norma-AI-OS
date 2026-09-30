import XCTest
@testable import NormaCore

final class ConversationDetailCardTests: XCTestCase {
    /// Codex命令与文件变更按真实字段展示，历史记录不能凭工具名称伪造成功。
    /// 数据构造：同一命令inProgress→completed/exit0→failed/exit7；700行“结果\n”=700×7字节。
    /// 执行过程：解析三种状态与文件变更→检查摘要/完整输出→缺少状态及未知状态→MCP返回isError。
    /// 预期结果：状态随正式记录变化，失败不显示完成；完整输出和diff保留，未知状态不猜测。
    func testCodexCommandsAndChangesKeepActualStateAndFullContent() throws {
        let output = String(repeating: "结果\n", count: 700)
        func command(_ state: String?, exit: Int? = nil) throws -> ConversationDetailCard {
            var record: [String: Any] = ["id": "cmd-1", "type": "commandExecution", "command": "swift test", "cwd": "/project", "aggregatedOutput": output]
            record["status"] = state; record["exitCode"] = exit
            return try XCTUnwrap(ConversationDetailCard.parse(json(record)).first, "正式命令应形成卡片")
        }
        let running = try command("inProgress")
        XCTAssertEqual(running.title, "运行命令", "不应继续用工具与附件泛化标签")
        XCTAssertEqual(running.status, "执行中", "只在Provider明确运行时展示执行中")
        XCTAssertEqual(running.summary, "swift test", "首屏应显示命令摘要")
        XCTAssertEqual(running.sections.first(where: { $0.title == "输出" })?.text, output, "700行结果必须完整保留到展开内容")
        XCTAssertEqual(try command("completed", exit: 0).status, "已完成", "明确完成且exit0才显示完成")
        let failed = try command("failed", exit: 7)
        XCTAssertTrue(failed.isFailure, "非零退出必须显示错误样式")
        XCTAssertNotEqual(failed.status, "已完成", "失败命令不能显示绿色成功")
        XCTAssertNil(try command(nil).status, "老记录没有状态时不能推断正在运行或已完成")
        XCTAssertNotEqual(try command("future-state").status, "已完成", "新协议未知状态不得冒充成功")
        let change = try XCTUnwrap(ConversationDetailCard.parse(json(["type": "fileChange", "status": "completed", "changes": [["path": "src/App.swift", "diff": "+new\n-old"]]])).first, "变更应形成卡片")
        XCTAssertEqual(change.title, "修改文件", "变更使用具体类型")
        XCTAssertEqual(change.summary, "src/App.swift", "首屏应显示受影响文件")
        XCTAssertTrue(change.sections.contains { $0.text.contains("+new\n-old") }, "差异不能被摘要替代")
        let mcp = try XCTUnwrap(ConversationDetailCard.parse(json(["type": "mcpToolCall", "tool": "search", "status": "completed", "result": ["isError": true, "content": [["type": "text", "text": "search failed"]]]])).first, "MCP返回必须可查看")
        XCTAssertTrue(mcp.isFailure, "工具正文明确isError不能被外层completed覆盖")
        XCTAssertNotEqual(mcp.status, "已完成", "传输完成不等于工具执行成功")
    }

    /// Claude混合工具块分别展示输入与结果，不把结果当作用户聊天或凭空匹配其他调用。
    /// 数据构造：Bash、Read各一个调用及失败结果，工具ID分别a/b；完整输入含中文与多行。
    /// 执行过程：解析details数组→逐项检查类型、状态、完整内容和稳定块序号。
    /// 预期结果：调用仅标已请求，结果只按is_error显示失败/已返回；调用ID和全部原始字段可查看。
    func testClaudeCallsAndResultsAreSimpleSeparateCards() throws {
        let cards = ConversationDetailCard.parse(try json([
            ["type": "tool_use", "id": "a", "name": "Bash", "input": ["command": "printf '第一行\n第二行'"]],
            ["type": "tool_use", "id": "b", "name": "Read", "input": ["file_path": "/project/README.md"]],
            ["type": "tool_result", "tool_use_id": "a", "is_error": true, "content": "permission denied"],
            ["type": "tool_result", "tool_use_id": "b", "is_error": false, "content": [["type": "text", "text": "完整文件内容"]]]
        ]))
        XCTAssertEqual(cards.count, 4, "四个原始工具块不能丢弃或误合并")
        guard cards.count == 4 else { return }
        XCTAssertEqual(cards[0].title, "Bash", "工具名应保留")
        XCTAssertEqual(cards[0].status, "已请求", "历史调用没有完成事件不能声称正在执行")
        XCTAssertFalse(cards[0].isFailure, "调用本身不是执行失败")
        XCTAssertEqual(cards[1].summary, "/project/README.md", "读取文件应显示目标路径")
        XCTAssertEqual(cards[2].status, "失败", "is_error必须明确呈现")
        XCTAssertTrue(cards[2].isFailure, "失败结果必须使用错误状态")
        XCTAssertEqual(cards[3].status, "已返回", "收到结果不代表整个任务成功")
        XCTAssertEqual(cards[3].sections.first?.text, "完整文件内容", "文本结果应直接可读，不被JSON转义包裹")
        XCTAssertEqual(cards[3].id, 3, "多工具同名也应按原块保持独立身份")
        XCTAssertTrue(cards[2].raw.contains("tool_use_id"), "原始关联字段不能被展示层丢弃")
    }

    /// 未知或损坏工具记录保持可读，不执行内容，也不丢失附件或把状态缺失当成功。
    /// 数据构造：未来工具块、字符串原文、非法JSON；摘要上限120字符，多余内容只折叠不删除。
    /// 执行过程：解析未知与非法记录→解析200字命令→读取完整原始记录。
    /// 预期结果：每种记录都有回退卡片，无成功状态；长摘要有省略号且完整命令仍在详情。
    func testUnknownAndLongDetailsRemainAvailableWithoutInventedSuccess() throws {
        let unknown = try XCTUnwrap(ConversationDetailCard.parse(json(["type": "futureTool", "secretField": "保留原字段"])).first, "未知记录也必须可查看")
        XCTAssertNil(unknown.status, "未知工具不能猜测执行成功")
        XCTAssertTrue(unknown.raw.contains("保留原字段"), "未知字段不能静默消失")
        let malformed = "{\"invalid\": 未完成"
        let fallback = try XCTUnwrap(ConversationDetailCard.parse(malformed).first, "非法JSON需保留原文")
        XCTAssertEqual(fallback.raw, malformed, "不能修补或丢弃错误原文")
        XCTAssertFalse(fallback.isFailure, "解码失败不等于工具执行失败")
        let long = String(repeating: "x", count: 200)
        let card = try XCTUnwrap(ConversationDetailCard.parse(json(["type": "commandExecution", "command": long])).first, "长工具输入仍要可用")
        XCTAssertEqual(card.summary, String(repeating: "x", count: 120) + "…", "首屏摘要应有明确省略提示")
        XCTAssertEqual(card.sections.first?.text, long, "只缩短首屏，不裁掉完整输入")
    }

    private func json(_ value: Any) throws -> String {
        String(decoding: try JSONSerialization.data(withJSONObject: value), as: UTF8.self)
    }
}

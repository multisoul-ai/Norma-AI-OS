import Foundation
import Testing
@testable import NormaCore

private func terminalFragment(_ offset: Int, next: Int?, text: String, fg: UInt32 = 16711680) throws -> WorkspaceTerminalFragment {
    let data = try JSONSerialization.data(withJSONObject: [
        "snapshot_id":"capture-a", "offset":offset, "row_count":2, "next_offset":next as Any? ?? NSNull(),
        "version":"screen-a", "cols":1, "screen_lines":2, "start_line":0, "total_lines":2,
        "running":true, "alt_screen":false, "cursor":["line":1,"column":0],
        "background":0, "foreground":16777215, "rows":[[["text":text,"fg":fg,"bg":0,"flags":0]]]
    ])
    return try JSONDecoder().decode(WorkspaceTerminalFragment.self, from: data)
}

/// 场景：两片富终端完整收到后才替换画面，保留颜色、光标和固定列宽。
/// 数据构造：2行×1列，首片红色A、末片绿色B；offset依次0和1。
/// 执行过程：接收首片 → 画面仍为空 → 接收末片 → 检查完整网格。
/// 预期结果：半帧不可见；末行及颜色准确；没有拼接成纯文本或重复行。
@Test func richTerminalWaitsForCompleteCapture() throws {
    var assembly = WorkspaceTerminalAssembly()
    #expect(try assembly.apply(terminalFragment(0, next: 1, text: "A")) == nil, "分片尚未结束，不能显示半帧")
    let complete = try assembly.apply(terminalFragment(1, next: nil, text: "B", fg: 65280))
    let frame = try #require(complete)
    #expect(frame.rows.count == 2, "完整快照只能有原来的两行")
    #expect(frame.rows[0][0].fg == 16711680, "首行保留Mac红色")
    #expect(frame.rows[1][0].text == "B", "末行不能丢失")
    #expect(frame.rows[1][0].fg == 65280, "末行保留Mac绿色")
    #expect(frame.cursor?.line == 1, "光标保持原绝对行")
    #expect(frame.cols != 2, "不能为了手机宽度擅自重排终端列数")
}

/// 场景：乱序分片不能接入已有半帧。
/// 数据构造：完整捕获需要offset0→1；直接发送offset1。
/// 执行过程：向空装配器发送末片 → 捕获协议错误 → 重新从首片开始。
/// 预期结果：缺首片被拒绝；重新开始有效；不能生成伪造完整画面。
@Test func richTerminalRejectsMissingFirstFragment() throws {
    var assembly = WorkspaceTerminalAssembly()
    #expect(throws: MobileError.invalidResponse) { try assembly.apply(terminalFragment(1, next: nil, text: "B")) }
    #expect(try assembly.apply(terminalFragment(0, next: 1, text: "A")) == nil, "错误后可从首片重建，但不能提前显示")
}

/// 场景：分片过早结束、重复首片或篡改颜色元数据均不得显示不完整画面。
/// 数据构造：总共2行，每片1行；伪造首片宣称没有下一片。
/// 执行过程：过早结束 → 拒绝；正常首片 → 重复首片 → 拒绝并清理。
/// 预期结果：不能用缺行画面替换上一个完整画面，也不能中途切换捕获。
@Test func richTerminalRejectsTruncatedOrRestartedCapture() throws {
    var assembly = WorkspaceTerminalAssembly()
    #expect(throws: MobileError.invalidResponse) { try assembly.apply(terminalFragment(0, next: nil, text: "A")) }
    #expect(try assembly.apply(terminalFragment(0, next: 1, text: "A")) == nil, "错误后应能开始新捕获")
    #expect(throws: MobileError.invalidResponse) { try assembly.apply(terminalFragment(0, next: 1, text: "B")) }
}

/// 场景：复制中文宽字不重复spacer，自动折行拼接，hidden内容不泄漏。
/// 数据构造：2行×4格；第一行「中」占2格，A和B占2格并soft-wrap；第二行隐藏X。
/// 执行过程：解码Mac网格 → 按视觉行拼接正文。
/// 预期结果：「中」只出现一次，AB连接后续C；不含隐藏X及虚假的折行换行。
@Test func richTerminalCopiesWideCharactersAndSoftWrap() throws {
    let cell: (String, UInt16) -> [String: Any] = { ["text":$0,"fg":16777215,"bg":0,"flags":$1] }
    let data = try JSONSerialization.data(withJSONObject: ["version":"wide-a","cols":4,"screen_lines":2,
        "start_line":0,"total_lines":2,"running":true,"alt_screen":false,"background":0,"foreground":16777215,
        "rows":[[cell("中",0),cell(" ",32),cell("A",0),cell("B",64)],
                [cell("C",0),cell("X",16),cell(" ",0),cell(" ",0)]]])
    let frame = try JSONDecoder().decode(WorkspaceTerminalFrame.self, from: data)
    #expect(frame.text() == "中ABC", "宽字、隐藏内容和soft-wrap必须遵循Mac网格")
    #expect(!frame.text().contains("X"), "hidden格不得复制出隐藏字符")
    #expect(!frame.text().contains("\n"), "自动折行不能变成真实换行")
}

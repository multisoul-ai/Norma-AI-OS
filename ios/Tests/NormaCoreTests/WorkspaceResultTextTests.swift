import Testing
@testable import NormaCore

/// 场景：成果中路径和代码被分别呈现，围栏语言标记不作为用户正文。
/// 数据构造：1段含内联路径的正文和1个text代码块；代码内部含Markdown特殊字符。
/// 执行过程：解析成果块 → 检查正文顺序及代码原文 → 检查围栏不会混入代码。
/// 预期结果：正文与代码分别保留；代码符号不被当Markdown；不闭合围栏的内容也不会丢失。
@Test func resultCodeFencesBecomeSeparateReadableBlocks() {
    let blocks = workspaceResultBlocks("Path: `/tmp/proof.txt`\n\n```text\nNorma **literal** ok.\n```\nFinished.")
    #expect(blocks == [.text("Path: `/tmp/proof.txt`\n"), .code("Norma **literal** ok."), .text("Finished.")], "正文与代码块应按来源顺序呈现")
    #expect(!blocks.contains(.code("```text")), "围栏不能显示为代码内容")
    #expect(workspaceResultBlocks("```swift\nlet x = 1") == [.code("let x = 1")], "部分流式代码未闭合也必须保留正文")
    #expect(workspaceResultBlocks("ordinary `inline` text") == [.text("ordinary `inline` text")], "内联标记交给原生AttributedString处理")
}

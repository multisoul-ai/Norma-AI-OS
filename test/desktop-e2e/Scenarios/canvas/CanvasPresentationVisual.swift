import E2EKit

/// 同一生产流程的多模态判据；CLI 身份、复用和数据断言仍在原场景执行。
@MainActor
enum CanvasPresentationVisual {
    private static let layoutCriteria = [
        VisualCriterion(id: "cards", text: "画布恰有六张 Agent 和两张终端；六张 Agent 的 LAYOUT AGENT 正文可读，两张不同终端分别有 TERMONE、TERMTWO 正文（不把命令回显或同一卡两行算两卡）"),
        VisualCriterion(id: "geometry", text: "1440×840 窗口：卡片平铺，首张 Agent 在左侧（正文 x<450），Agent 分布跨行（y 跨度>200），末行两张终端横距>150；所有可见卡片不是细条，无隐藏卡片留下的首格或整列空档")
    ]

    static func layout(_ session: Session, content: String? = nil, browser: Bool = false) async throws -> Frame {
        var criteria = layoutCriteria
        if let content {
            criteria.append(VisualCriterion(id: "content", text: "右侧内容抽屉已实际展示\(browser ? "网页" : "文件预览")，正文包含 \(content)；内容不是画布主卡片，正文未被空白、错误页或加载占位替代"))
        }
        return try await session.visualCheck(criteria)
    }

    static func master(_ session: Session) async throws {
        try await session.visualClick("定位 TERMONE 终端正文，以将这张可见终端选为 Master")
        try await session.shortcut(key: 53, modifiers: [])
        try await session.shortcut(key: 46, modifiers: [0x37])
        let frame = try await session.visualCheck([
            VisualCriterion(id: "master", text: "TERMONE 终端占左侧宽主区域，正文 x<450；主区域不为空白"),
            VisualCriterion(id: "details", text: "右侧 x>700 的 detail 保留六张 Agent 和 TERMTWO 终端，七张正文都可见；这是合法 Master 的主次布局")
        ])
        try session.save(frame, "01-visible-master.png")
        try await session.shortcut(key: 46, modifiers: [0x37])
        _ = try await layout(session)
    }
}

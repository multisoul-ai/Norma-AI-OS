import XCTest
import UIKit
@testable import Norma
@testable import NormaCore

@MainActor final class WorkspaceTerminalGridTests: XCTestCase {
    /// 场景：详情转场期间懒加载消息高度变化，返回仍保持原执行卡在视口的位置。
    /// 数据构造：视口高500pt；卡片原y=600、offset=200，视口y=600-200=400。
    ///   重排后卡片y=900，因此恢复offset=900-400=500，不能使用旧200pt偏移。
    /// 执行过程：记录原卡锚点 → 模拟长消息重排 → 恢复阅读位置。
    /// 预期结果：卡片仍在视口400pt；横向位置保留；原偏移200不得被误用。
    func testChatReadingAnchorSurvivesMessageRelayout() {
        let scroll = UIScrollView(frame: CGRect(x: 0, y: 0, width: 300, height: 500))
        scroll.contentSize = CGSize(width: 500, height: 1500)
        let card = UIView(frame: CGRect(x: 0, y: 600, width: 300, height: 50)); scroll.addSubview(card)
        scroll.contentOffset = CGPoint(x: 4, y: 200)
        let reading = WorkspaceChatReadingPosition()
        reading.bind(scrollView: scroll, anchorID: 24, view: card)
        reading.preserve(anchorID: 24)
        card.frame.origin.y = 900
        reading.restore()
        XCTAssertEqual(scroll.contentOffset.y, 500, accuracy: 0.1, "重排后应按原卡位置恢复，而不是沿用旧像素偏移")
        XCTAssertEqual(card.convert(.zero, to: scroll).y - scroll.contentOffset.y, 400, accuracy: 0.1, "原卡必须保留原视口位置")
        XCTAssertEqual(scroll.contentOffset.x, 4, accuracy: 0.1, "纵向重排不能丢失横向位置")
        XCTAssertNotEqual(scroll.contentOffset.y, 200, "旧偏移不能冒充正确恢复")
    }

    private func frame(text: String = "中", color: UInt32 = 0xFF0000, cursorLine: Int = 1, rows: Int = 2) throws -> WorkspaceTerminalFrame {
        let row: [[String: Any]] = [["text":text,"fg":color,"bg":0,"flags":0], ["text":" ","fg":color,"bg":0,"flags":32]]
        return try JSONDecoder().decode(WorkspaceTerminalFrame.self, from: JSONSerialization.data(withJSONObject: [
            "version":"version-a","cols":2,"screen_lines":min(rows,24),"start_line":0,"total_lines":rows,"running":true,
            "alt_screen":false,"cursor":["line":cursorLine,"column":0],"background":0,"foreground":16777215,"rows":Array(repeating: row,count:rows)]))
    }

    /// 场景：原位进度、颜色和光标变化会刷新对应网格行，中文spacer不变成新字符。
    /// 数据构造：2行×2格，中文占2格；红→绿，只移动光标第1行→第0行。
    /// 执行过程：绘制首帧 → 同位置改进度文本及颜色 → 只移动光标 → 重复相同画面。
    /// 预期结果：变更行和新旧光标行刷新；相同帧不刷新；可访问文字不包含spacer。
    func testProgressColorAndCursorInvalidateFixedRows() throws {
        let grid = TerminalGridCanvas(frame: CGRect(x: 0, y: 0, width: 100, height: 100))
        grid.apply(try frame(), fontSize: 13)
        grid.apply(try frame(text: "好", color: 0x00FF00), fontSize: 13)
        XCTAssertEqual(grid.changedRows, [0, 1], "颜色和原位字符改变必须刷新两行")
        XCTAssertEqual(grid.terminal?.rows[0][0].fg, 0x00FF00, "CoreText应使用Mac已解析的绿颜色")
        grid.apply(try frame(text: "好", color: 0x00FF00, cursorLine: 0), fontSize: 13)
        XCTAssertEqual(grid.changedRows, [0, 1], "移动光标必须擦除旧行并画出新行")
        XCTAssertEqual(grid.accessibilityValue, "好\n好", "VoiceOver正文不应包含宽字spacer")
        grid.apply(try frame(text: "好", color: 0x00FF00, cursorLine: 0), fontSize: 13)
        XCTAssertTrue(grid.changedRows.isEmpty, "无变化画面不能伪造刷新区域")
        XCTAssertFalse(grid.terminal?.cols == 1, "手机不得把中文宽字网格压成一列")
    }

    /// 场景：字号缩放保留阅读行，返回实时只改变手机滚动，不重排Mac列宽。
    /// 数据构造：2列×120行；视口100pt高，阅读位置为第10行；字号13→18。
    /// 执行过程：长网格滚到底 → 暂停阅读第10行 → 放大字号 → 回到实时。
    /// 预期结果：暂停位置仍为第10行；回实时到手机底部；固定2列保持。
    func testFontScalingPreservesPausedReadingAndReturnsToLatest() throws {
        let capture = try frame(rows: 120)
        let view = TerminalGridScrollView(frame: CGRect(x: 0, y: 0, width: 100, height: 100))
        view.apply(capture, fontSize: 13, following: true, returnGeneration: 0)
        view.contentOffset.y = 10 * view.grid.cellSize.height
        view.apply(capture, fontSize: 18, following: false, returnGeneration: 0)
        XCTAssertEqual(view.contentOffset.y / view.grid.cellSize.height, 10, accuracy: 0.1, "缩放字号不能丢失原阅读行")
        view.apply(capture, fontSize: 18, following: true, returnGeneration: 1)
        XCTAssertEqual(view.contentOffset.y, view.contentSize.height - view.bounds.height, accuracy: 0.1, "回到实时必须定位最新输出")
        XCTAssertEqual(view.grid.terminal?.cols, 2, "UIFont变化不应改变Mac终端列宽")
        XCTAssertFalse(view.isFirstResponder, "只读画面不能获取终端键盘输入权")
    }

    /// 场景：离开终端标签后重新创建UIKit视图，保留原阅读行和横向位置。
    /// 数据构造：120行网格，读第10行且横向偏移4pt；同Sheet共用位置引用。
    /// 执行过程：原视图记录位置 → 新建终端视图 → 应用同历史页。
    /// 预期结果：仍读第10行及原列位置，不跳到页底或归零。
    func testRecreatingTerminalTabRestoresReadingPosition() throws {
        let capture = try frame(rows: 120)
        let reading = TerminalGridReadingPosition()
        let original = TerminalGridScrollView(frame: CGRect(x: 0, y: 0, width: 100, height: 100)); original.reading = reading
        original.apply(capture, fontSize: 13, following: false, returnGeneration: 0)
        original.contentOffset = CGPoint(x: 4, y: 10 * original.grid.cellSize.height); original.saveReadingPosition()
        let returned = TerminalGridScrollView(frame: original.frame); returned.reading = reading
        returned.apply(capture, fontSize: 13, following: false, returnGeneration: 0)
        XCTAssertEqual(returned.contentOffset.y / returned.grid.cellSize.height, 10, accuracy: 0.1, "切标签后必须保留阅读行")
        XCTAssertEqual(returned.contentOffset.x, 4, accuracy: 0.1, "切标签后必须保留横向阅读位置")
        XCTAssertNotEqual(returned.contentOffset.y, returned.contentSize.height - returned.bounds.height, "恢复不能误跳至页底")
    }
}

import XCTest
@testable import NormaCore

final class ApprovalSwipeTests: XCTestCase {
    /// 左右决策沿用原型实际位移阈值；短拖、竖滑、无效输入和取消不能授权。
    ///
    /// 数据构造：320×0.28=89.6pt；窄卡阈值下限88pt，宽卡上限118pt；横向须大于竖向的1.25倍。
    /// 执行过程：短拖→到阈值→反向；新手势竖滑后横移；检查取消、非有限数及宽度边界。
    /// 预期结果：右移允许、左移拒绝；方向锁定/取消不可复活，不使用速度预测补足距离。
    func testDistanceDirectionAndCancellation() {
        var swipe = ApprovalSwipe()
        swipe.update(dx: 89.5, dy: 0, width: 320)
        XCTAssertNil(swipe.decision, "阈值前即使速度很快也不能授权")
        swipe.update(dx: 320 * 0.28, dy: 0, width: 320) // 使用同一浮点乘法表示精确边界89.6。
        XCTAssertEqual(swipe.decision, .allow, "达到原型阈值才允许一次")
        swipe.update(dx: -90, dy: 0, width: 320)
        XCTAssertEqual(swipe.decision, .deny, "松手前反向必须更新为拒绝")
        swipe.cancel()
        swipe.update(dx: 200, dy: 0, width: 320)
        XCTAssertNil(swipe.decision, "失焦或来源变化后不能复活旧手势")
        var vertical = ApprovalSwipe()
        vertical.update(dx: 0, dy: 9, width: 320) // 9 > 8pt 竖向意图门槛。
        vertical.update(dx: 200, dy: 10, width: 320)
        XCTAssertNil(vertical.decision, "先竖滑再横移仍只滚动")
        var diagonal = ApprovalSwipe()
        diagonal.update(dx: 100, dy: 80, width: 320) // 100 = 80×1.25，等于边界仍不提交。
        XCTAssertNil(diagonal.decision, "对角边界不是明确横向决策")
        var narrow = ApprovalSwipe()
        narrow.update(dx: 87, dy: 0, width: 200)
        XCTAssertNil(narrow.decision, "200×0.28=56，小于下限88，不能过早提交")
        narrow.update(dx: 88, dy: 0, width: 200)
        XCTAssertEqual(narrow.decision, .allow, "窄卡使用88pt下限")
        var wide = ApprovalSwipe()
        wide.update(dx: -118, dy: 0, width: 800) // 800×0.28=224，取118pt上限。
        XCTAssertEqual(wide.decision, .deny, "宽卡无需拖过118pt")
        var invalid = ApprovalSwipe()
        invalid.update(dx: .nan, dy: 0, width: 320)
        invalid.update(dx: 200, dy: 0, width: 320)
        XCTAssertNil(invalid.decision, "非有限坐标必须取消整次手势")
        var infinity = ApprovalSwipe()
        infinity.update(dx: 200, dy: .infinity, width: 320)
        XCTAssertNil(infinity.decision, "无限竖向坐标不能成为有效批准")
        var zero = ApprovalSwipe()
        zero.update(dx: 200, dy: 0, width: 0)
        XCTAssertNil(zero.decision, "未布局的卡片不能授权")
    }
}

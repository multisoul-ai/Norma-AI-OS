use super::*;
use gpui::TestAppContext;
use std::time::Duration;

/// B-16：Browser 地址栏 caret 聚焦后应以 500ms 周期交替显示与隐藏。
///
/// 数据构造（含关键数值的推导过程）：
///   - blink interval = 500ms，沿用桌面文本输入的常见半秒节奏。
///   - t=0ms 时 caret 可见；推进 499ms 后尚未到第一个边界，仍应可见。
///   - 再推进 1ms：499 + 1 = 500ms，caret 应隐藏。
///   - 再推进 500ms：500 + 500 = 1000ms，caret 应重新显示。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 headless GPUI 窗口中创建真实 BrowserAddressInput 实体。
///   2. 使用 mock executor 分别推进到 499ms、500ms 和 1000ms。
///   3. 每次推进后驱动 pending task，并读取 caret 可见状态。
///   4. 检查生产 painter 确实用该状态控制 caret，而不是继续无条件绘制。
///
/// 预期结果：
///   - 正断言：0ms 与 499ms 时 caret 可见，避免刚聚焦就消失。
///   - 正断言：500ms 时 caret 隐藏，证明存在动态 blink。
///   - 正断言：1000ms 时 caret 恢复可见，证明动画会持续循环。
///   - 正断言：render 层读取 caret 可见状态，定时变化能到达用户画面。
///   - 负断言：500ms 与 1000ms 状态不得相同，否则 caret 仍是静态效果。
#[test]
fn test_browser_address_caret_blinks_every_500ms() {
    let mut test_cx = TestAppContext::single();
    let (input, cx) = test_cx.add_window_view(|_window, cx| {
        BrowserAddressInput::new("https://example.com".to_string(), Theme::glass_dark(), cx)
    });

    let visible_at_start = input.read_with(cx, |input, _| input.caret_visible);
    assert!(
        visible_at_start,
        "caret 在地址栏刚创建时必须立即可见，不能先等待半秒"
    );

    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(499));
    cx.run_until_parked();
    let visible_at_499ms = input.read_with(cx, |input, _| input.caret_visible);
    assert!(
        visible_at_499ms,
        "499ms 尚未到 blink 边界，caret 必须保持可见"
    );

    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    let visible_at_500ms = input.read_with(cx, |input, _| input.caret_visible);
    assert!(
        !visible_at_500ms,
        "500ms 到达首个 blink 边界后 caret 必须隐藏"
    );

    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    let visible_at_1000ms = input.read_with(cx, |input, _| input.caret_visible);
    assert!(
        visible_at_1000ms,
        "1000ms 到达第二个 blink 边界后 caret 必须重新显示"
    );
    assert_ne!(
        visible_at_500ms, visible_at_1000ms,
        "相邻 blink 边界的 caret 可见状态不得相同，否则动画仍是静态效果"
    );

    let render_source = include_str!("../../src/app/browser/address_input/render.rs");
    assert!(
        render_source.contains("input.caret_visible.then"),
        "caret painter 必须由 blink 可见状态控制，否则实体状态变化不会产生视觉动画"
    );
}

/// B-17：用户输入后 caret 应立即恢复可见，并暂停 300ms 后再继续闪烁。
///
/// 数据构造（含关键数值的推导过程）：
///   - 先推进 500ms，使 caret 从可见切换到隐藏，构造最严格的交互起点。
///   - 平台文本输入 = `x`，会替换初始全选 URL，并把选区折叠到 byte offset 1。
///   - pause delay = 300ms；输入后 299ms 仍应可见，299 + 1 = 300ms 时恢复 blink 并隐藏。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建真实输入实体并把 mock clock 推进到首个隐藏相位。
///   2. 通过 EntityInputHandler 的平台文本入口写入 `x`。
///   3. 立即读取 caret，再分别推进 299ms 与 1ms。
///
/// 预期结果：
///   - 正断言：输入前 caret 确实处于隐藏相位，避免测试从可见态假绿。
///   - 正断言：输入后和 299ms 时 caret 可见，用户能立刻确认输入位置。
///   - 正断言：300ms 时 caret 再次隐藏，证明暂停结束后动画恢复。
///   - 负断言：输入前后可见状态不得相同，否则交互没有重置 blink。
#[test]
fn test_browser_address_input_restarts_caret_blink_after_300ms_pause() {
    let mut test_cx = TestAppContext::single();
    let (input, cx) = test_cx.add_window_view(|_window, cx| {
        BrowserAddressInput::new("https://example.com".to_string(), Theme::glass_dark(), cx)
    });

    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    let hidden_before_input = input.read_with(cx, |input, _| input.caret_visible);
    assert!(
        !hidden_before_input,
        "测试前置必须把 caret 推进到隐藏相位，才能验证输入会重置 blink"
    );

    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "x", window, cx);
        });
    });
    let visible_after_input = input.read_with(cx, |input, _| input.caret_visible);
    assert!(visible_after_input, "平台文本输入后 caret 必须立即恢复可见");

    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(299));
    cx.run_until_parked();
    let visible_at_299ms = input.read_with(cx, |input, _| input.caret_visible);
    assert!(
        visible_at_299ms,
        "输入后的 299ms 暂停窗口内 caret 必须保持可见"
    );

    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    let visible_at_300ms = input.read_with(cx, |input, _| input.caret_visible);
    assert!(
        !visible_at_300ms,
        "输入后满 300ms 应恢复 blink，并进入隐藏相位"
    );
    assert_ne!(
        hidden_before_input, visible_after_input,
        "输入前后的 caret 可见状态必须不同，证明交互确实重置了 blink"
    );
}

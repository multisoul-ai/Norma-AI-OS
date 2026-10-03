use crate::app::MultisoulApp;
use gpui::{
    point, px, size, AppContext, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase,
};

/// 场景：Norma 语音设置的长页面能通过滚轮访问末尾的声音配置。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离空数据根，无项目、无已安装语音模型，使用真实 Voice Setup 页面。
///   窗口 = 1280×800px（桌面基准），内容可用高度扣除设置边距与固定标题。
///   滚动距离 = 布局实际计算出的 max_offset.height，不使用猜测的滚动阈值。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Norma → 语音并绘制真实布局，记录内容视口与 Spoken Replies 位置。
///   2. 在内容视口发送滚轮事件，向下滚动整个实际可滚动距离。
///   3. 重绘并检查末尾配置进入视口。
///
/// 预期结果：
///   - 正断言：长内容有正的滚动范围；滚轮后声音配置进入视口。
///   - 负断言：初始末尾配置不在视口；滚动后偏移不能仍为零。
#[test]
fn norma_voice_settings_scrolls_to_spoken_replies() {
    let temp = tempfile::tempdir().expect("应创建独立测试目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    let host = host.expect("应创建真实设置宿主");
    host.update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("应有内容视口");
    let before = cx
        .debug_bounds("voice-setup-requirement-tts")
        .expect("应渲染声音配置");
    assert!(
        before.top() >= viewport.bottom(),
        "长页面末尾应初始位于视口之外"
    );
    let scroll = host.update(cx, |app, _| app.settings_viewport.content_scroll.clone());
    let distance = scroll.max_offset().height;
    assert!(
        distance > px(0.0),
        "语音长页面必须有真实滚动范围，不能被固定高度容器截断"
    );
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), -distance)),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let after = cx
        .debug_bounds("voice-setup-requirement-tts")
        .expect("滚动后应保留声音配置");
    assert!(
        scroll.offset().y < px(0.0),
        "滚轮必须改变偏移，不能停留在页首"
    );
    assert!(
        after.top() < viewport.bottom() && after.bottom() > viewport.top(),
        "Spoken Replies 必须能进入视口并供用户操作"
    );
}

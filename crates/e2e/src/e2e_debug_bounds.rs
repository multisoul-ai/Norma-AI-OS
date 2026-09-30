//! The vendored GPUI selector map must describe the current frame, including cached views.
use gpui::{
    div, prelude::*, AnyView, AppContext, Context, Entity, StyleRefinement, TestAppContext, Window,
};
use std::{cell::Cell, rc::Rc};

struct CachedLabel(Rc<Cell<usize>>);
impl Render for CachedLabel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.set(self.0.get() + 1);
        div()
            .debug_selector(|| "cached-label".into())
            .child("Cached")
    }
}
struct ChangingLabel {
    old: bool,
    cached: Entity<CachedLabel>,
}
impl Render for ChangingLabel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let name = if self.old { "old-label" } else { "new-label" };
        div()
            .flex()
            .flex_col()
            .child(div().debug_selector(move || name.into()).child(name))
            .child(AnyView::from(self.cached.clone()).cached(StyleRefinement::default()))
    }
}

/// 当前帧检查必须丢弃已移除的按钮，同时保留没有重新渲染的缓存子视图。
///
/// 数据构造：2 个可见元素 = 1 个切换标签 + 1 个缓存标签；先画 2 次旧帧，再画 3 次新帧覆盖双缓冲复用。
/// 执行过程：先绘制 old/cached；切换到 new，再通知根视图两次，读取最终边界与缓存渲染计数。
/// 预期结果：new/cached 存在，old 不存在；缓存仍只渲染 1 次，不能靠关闭缓存修复检查。
#[test]
fn debug_bounds_tracks_removal_and_cached_paint() {
    let mut cx = TestAppContext::single();
    let renders = Rc::new(Cell::new(0));
    let count = renders.clone();
    let (root, cx) = cx.add_window_view(|_, cx| ChangingLabel {
        old: true,
        cached: cx.new(|_| CachedLabel(count)),
    });
    assert!(
        cx.debug_bounds("old-label").is_some(),
        "初始标签必须真实绘制"
    );
    assert!(cx.debug_bounds("new-label").is_none(), "新标签尚未出现");
    root.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    root.update(cx, |root, cx| {
        root.old = false;
        cx.notify();
    });
    cx.run_until_parked();
    // 额外 2 帧 + 切换帧 = 3 帧，覆盖 rendered/next 两份帧的回收。
    for _ in 0..2 {
        root.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    assert!(
        cx.debug_bounds("new-label").is_some(),
        "最终帧必须包含新标签"
    );
    assert!(
        cx.debug_bounds("old-label").is_none(),
        "已移除标签不能残留在 debug_bounds"
    );
    assert!(
        cx.debug_bounds("cached-label").is_some(),
        "重放缓存绘制时必须保留标签边界"
    );
    assert_eq!(renders.get(), 1, "缓存子视图不应因检查边界而重新渲染");
}

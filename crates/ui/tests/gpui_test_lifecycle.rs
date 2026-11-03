//! Test infrastructure must release window-owned resources even after a failed assertion.
use gpui::{Context, IntoElement, Render, TestAppContext, Window};
use std::{
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

struct WindowResource(Arc<AtomicBool>);
impl Drop for WindowResource {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
impl Render for WindowResource {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
    }
}

/// 场景：手写GPUI测试离开作用域时，窗口不能保留实体及其后台资源。
/// 数据构造：真实TestAppContext窗口、实体Drop见证和AppCell弱引用。
/// 执行过程：创建窗口后只让根测试上下文离开作用域，不手工调用quit。
/// 预期结果：窗口实体已销毁，AppCell没有残留强引用；不能跨测试累计线程。
#[test]
fn root_test_context_drop_releases_window_resources() {
    let released = Arc::new(AtomicBool::new(false));
    let weak_app = {
        let mut test = TestAppContext::single();
        let weak = Rc::downgrade(&test.app);
        let (_, _visual) = test.add_window_view(|_, _| WindowResource(released.clone()));
        assert!(
            !released.load(Ordering::Acquire),
            "存活窗口必须继续拥有实体资源"
        );
        weak
    };
    assert!(
        released.load(Ordering::Acquire),
        "根测试上下文离开作用域必须关闭窗口并释放实体"
    );
    assert!(
        weak_app.upgrade().is_none(),
        "VisualTestContext不能在测试结束后保留AppCell"
    );
}

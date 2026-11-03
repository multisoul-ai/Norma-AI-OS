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

/// 场景：VisualTestContext等克隆不能提前关闭根测试窗口，显式退出后析构应幂等。
/// 数据构造：窗口资源、根上下文克隆及退出计数器。
/// 执行过程：丢弃克隆→检查窗口仍存活→两次显式quit→根上下文析构。
/// 预期结果：克隆不释放窗口，所有退出路径合计只运行一次清理回调。
#[test]
fn clones_do_not_own_shutdown_and_explicit_quit_is_idempotent() {
    let released = Arc::new(AtomicBool::new(false));
    let cleanup_count = Rc::new(std::cell::Cell::new(0));
    {
        let mut test = TestAppContext::single();
        let count = cleanup_count.clone();
        test.on_quit(move || count.set(count.get() + 1));
        let (_, _visual) = test.add_window_view(|_, _| WindowResource(released.clone()));
        drop(test.clone());
        assert!(
            !released.load(Ordering::Acquire),
            "克隆退出不能提前销毁存活窗口"
        );
        test.quit();
        test.quit();
        assert!(released.load(Ordering::Acquire), "显式退出必须释放窗口");
    }
    assert_eq!(cleanup_count.get(), 1, "显式退出和根析构不能重复清理");
}

/// 场景：断言失败展开时仍须释放窗口，不把单个失败变成后续线程资源耗尽。
/// 数据构造：真实窗口资源与受控原始panic标记。
/// 执行过程：窗口创建后panic，在外层catch_unwind观察清理和原始错误。
/// 预期结果：窗口已释放，原始panic仍然报告，不吞掉失败。
#[test]
fn failing_test_unwind_still_releases_window_resources() {
    let released = Arc::new(AtomicBool::new(false));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut test = TestAppContext::single();
        let (_, _visual) = test.add_window_view(|_, _| WindowResource(released.clone()));
        std::panic::panic_any("original test failure");
    }));
    let original = result.expect_err("受控原始失败必须继续向上传播");
    assert_eq!(
        original.downcast_ref::<&str>(),
        Some(&"original test failure"),
        "清理不能覆盖原始失败原因"
    );
    assert!(released.load(Ordering::Acquire), "panic展开仍须释放窗口");
}

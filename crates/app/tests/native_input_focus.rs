//! Runs AppKit on the process main thread, without opening a user window.
#[cfg(target_os = "macos")]
#[path = "../src/app/native_input.rs"]
pub(crate) mod native_input;
#[cfg(target_os = "macos")]
#[path = "../src/app/browser/scene_clip.rs"]
mod scene_clip;
#[cfg(target_os = "macos")]
mod app {
    pub(crate) use crate::native_input;
}

#[cfg(target_os = "macos")]
fn main() {
    tracing_subscriber::fmt().with_target(true).init();
    objc2::rc::autoreleasepool(|_| {
        // Keep the GPUI adapters type-checked alongside the AppKit regression.
        let _ = native_input::focus_gpui;
        let _ = native_input::mouse_focus_guard;
        let _ = scene_clip::NativeSceneClip::present;
        test_release_input_recovers_window_responder();
        test_hide_and_drop_release_input();
    });
}

/// NIF-2: 浏览器隐藏或移除前归还键盘响应者，且隐藏不能抢走其他编辑器焦点。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个未显示 NSWindow、1 个宿主 NSView、2 个 NSTextView；
///   400 × 300 points 为合法布局，无超时、阈值或用户数据。
/// 执行过程（逐步说明系统如何处理）：
///   1. 聚焦 clip 内编辑器 → 隐藏 clip → 检查宿主接收键盘输入。
///   2. 聚焦同窗独立编辑器 → 再隐藏 clip → 检查独立编辑器仍聚焦。
///   3. 显示并聚焦 clip → drop clip → 检查宿主接收键盘且容器已脱离视图树。
///   4. 创建新 clip 并聚焦 → 先 detach 内容再 drop clip，复现 Wry 的字段释放顺序。
///
/// 预期结果：
///   - 正断言：隐藏、直接移除及内容先移除都恢复宿主，独立编辑器仍可输入。
///   - 负断言：已隐藏浏览器不持有焦点，移除后容器不存在于宿主树。
#[cfg(target_os = "macos")]
fn test_hide_and_drop_release_input() {
    use objc2::{rc::Retained, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSBackingStoreType, NSTextView, NSView, NSWindow, NSWindowStyleMask};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let mtm = MainThreadMarker::new().expect("AppKit regression must run on the main thread");
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0)),
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    let parent = NSView::new(mtm);
    window.setContentView(Some(&parent));
    let browser = NSTextView::new(mtm);
    let clip = scene_clip::NativeSceneClip::new(&browser);
    let container = unsafe { browser.superview() }.expect("clip must own the browser");
    parent.addSubview(&container);
    clip.set_visible(true);
    assert!(
        window.makeFirstResponder(Some(&browser)),
        "browser must accept focus before hiding"
    );
    clip.set_visible(false);
    let responder = window
        .firstResponder()
        .expect("hiding must retain a keyboard responder");
    assert_eq!(
        Retained::as_ptr(&responder).cast::<()>(),
        Retained::as_ptr(&parent).cast::<()>(),
        "hiding browser must restore host focus before AppKit hides its editor"
    );
    assert_ne!(
        Retained::as_ptr(&responder).cast::<()>(),
        Retained::as_ptr(&browser).cast::<()>(),
        "hidden browser must not retain native keyboard focus"
    );
    let sibling = NSTextView::new(mtm);
    parent.addSubview(&sibling);
    assert!(
        window.makeFirstResponder(Some(&sibling)),
        "sibling must accept focus"
    );
    clip.set_visible(false);
    assert_eq!(
        Retained::as_ptr(&window.firstResponder().expect("sibling focus must exist")).cast::<()>(),
        Retained::as_ptr(&sibling).cast::<()>(),
        "hiding browser must preserve another native editor"
    );
    clip.set_visible(true);
    assert!(
        window.makeFirstResponder(Some(&browser)),
        "browser must accept focus before removal"
    );
    drop(clip);
    assert_eq!(
        Retained::as_ptr(
            &window
                .firstResponder()
                .expect("host focus must exist after removal")
        )
        .cast::<()>(),
        Retained::as_ptr(&parent).cast::<()>(),
        "removing browser must restore host focus before detaching the native view"
    );
    assert!(
        unsafe { container.superview() }.is_none(),
        "dropped clip must no longer exist in the host view tree"
    );
    let clip = scene_clip::NativeSceneClip::new(&browser);
    let container = unsafe { browser.superview() }.expect("new clip must own the browser");
    parent.addSubview(&container);
    clip.set_visible(true);
    assert!(
        window.makeFirstResponder(Some(&browser)),
        "browser must accept focus before backend destruction"
    );
    // Wry's WebView is dropped before its scene_clip and removes the content first.
    browser.removeFromSuperview();
    assert!(
        browser.window().is_none(),
        "backend destruction must detach content from its native window"
    );
    drop(clip);
    assert_eq!(
        Retained::as_ptr(
            &window
                .firstResponder()
                .expect("host focus must exist after backend destruction")
        )
        .cast::<()>(),
        Retained::as_ptr(&parent).cast::<()>(),
        "clip must restore host even when backend already detached its content"
    );
    assert_ne!(
        Retained::as_ptr(
            &window
                .firstResponder()
                .expect("responder must remain alive")
        )
        .cast::<()>(),
        Retained::as_ptr(&browser).cast::<()>(),
        "detached backend must not retain keyboard focus"
    );
    assert!(
        unsafe { container.superview() }.is_none(),
        "clip must detach after backend-first destruction"
    );
    println!("PASS NIF-2: hide and drop restore host; sibling editor preserved");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("native_input_focus: macOS-only AppKit test skipped");
}

/// NIF-1: 浏览器失去原生响应者后，归还输入必须恢复宿主视图且保留其他编辑器焦点。
///
/// 数据构造（含关键数值的推导过程）：
///   使用 1 个未显示的真实 NSWindow、1 个宿主 NSView、2 个 NSTextView；
///   第一个 NSTextView 置于 NativeSceneClip 内，第二个为同窗独立编辑器；
///   窗口 400 × 300 points，仅提供合法原生视图布局，不参与阈值判定。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 聚焦浏览器编辑器，再 makeFirstResponder(None)，模拟原生视图失焦后落回窗口。
///   2. 调用实际 NativeSceneClip::release_input，检查响应者恢复为宿主视图。
///   3. 聚焦独立编辑器，再调用 release_input，检查没有抢走同窗其他输入焦点。
///
/// 预期结果：
///   - 正断言：落回窗口后能恢复宿主响应者。
///   - 负断言：响应者不再是 NSWindow，也不会抢走独立编辑器的焦点。
#[cfg(target_os = "macos")]
fn test_release_input_recovers_window_responder() {
    use objc2::{rc::Retained, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSBackingStoreType, NSTextView, NSView, NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let mtm = MainThreadMarker::new().expect("AppKit regression must run on the main thread");
    let _app = NSApplication::sharedApplication(mtm);
    // 400 × 300 points accommodates two native editors; this window is never ordered on screen.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0)),
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    let parent = NSView::new(mtm);
    window.setContentView(Some(&parent));
    let browser = NSTextView::new(mtm);
    let clip = scene_clip::NativeSceneClip::new(&browser);
    let container = unsafe { browser.superview() }.expect("clip must own the native editor");
    parent.addSubview(&container);
    clip.set_visible(true);
    assert!(
        window.makeFirstResponder(Some(&browser)),
        "browser editor must accept native focus"
    );
    assert!(
        window.makeFirstResponder(None),
        "AppKit must allow focus to fall back to NSWindow"
    );
    assert_eq!(
        Retained::as_ptr(&window.firstResponder().expect("window fallback must exist"))
            .cast::<()>(),
        Retained::as_ptr(&window).cast::<()>(),
        "regression setup must reproduce the NSWindow fallback, not browser focus"
    );
    clip.release_input();
    let responder = window
        .firstResponder()
        .expect("restored window must have a responder");
    assert_eq!(
        Retained::as_ptr(&responder).cast::<()>(),
        Retained::as_ptr(&parent).cast::<()>(),
        "release_input must recover the host even when firstResponder is no longer inside the browser"
    );
    assert_ne!(
        Retained::as_ptr(&responder).cast::<()>(),
        Retained::as_ptr(&window).cast::<()>(),
        "NSWindow must not retain keyboard input after browser release"
    );

    let sibling = NSTextView::new(mtm);
    parent.addSubview(&sibling);
    assert!(
        window.makeFirstResponder(Some(&sibling)),
        "independent editor must accept focus"
    );
    clip.release_input();
    let responder = window
        .firstResponder()
        .expect("independent editor must remain focused");
    assert_eq!(
        Retained::as_ptr(&responder).cast::<()>(),
        Retained::as_ptr(&sibling).cast::<()>(),
        "browser release must preserve an independent editor's native focus"
    );
    assert_ne!(
        Retained::as_ptr(&responder).cast::<()>(),
        Retained::as_ptr(&parent).cast::<()>(),
        "browser release must not steal focus from a sibling editor"
    );
    println!("PASS NIF-1: orphaned responder recovered; sibling editor preserved");
}

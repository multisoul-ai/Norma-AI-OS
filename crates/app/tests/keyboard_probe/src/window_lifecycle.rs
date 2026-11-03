//! Actual GPUI/Win32 window lifecycle regression; no WebView or App fixture.
#[cfg(windows)]
mod native {
    use gpui::{
        div, prelude::*, px, size, AppContext, Application, Bounds, Context, Timer, Window,
        WindowBounds, WindowOptions,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{cell::Cell, rc::Rc, sync::Mutex, time::Duration};
    use windows::Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{IsWindow, SendMessageW, WM_ACTIVATE, WM_CLOSE},
    };

    static ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    struct Capture;
    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.level() == log::Level::Error
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                let message = format!("{}: {}", record.target(), record.args());
                eprintln!("{message}");
                ERRORS.lock().expect("error capture lock").push(message);
            }
        }
        fn flush(&self) {}
    }

    struct Probe;
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child("Native lifecycle regression")
        }
    }

    /// 场景：原生关窗、激活通知排队后关窗、App主动移除三条路径都必须完整收口。
    /// 数据构造：一个实际500×400 GPUI窗，无WebView；原生消息在同一UI任务内发送。
    /// 执行过程：等待250毫秒让窗口初始化，再触发指定路径；关窗回调后保留100毫秒排空队列。
    /// 预期结果：只关闭一次，HWND真实销毁，GPUI不再访问已移除窗口或重复销毁句柄。
    pub(super) fn run() {
        let mode = std::env::args().nth(1).expect("lifecycle mode required");
        assert!(
            ["native-close", "queued-active-close", "remove-window"].contains(&mode.as_str()),
            "unknown lifecycle mode: {mode}"
        );
        log::set_logger(&Capture).expect("install native error capture");
        log::set_max_level(log::LevelFilter::Error);
        let closed = Rc::new(Cell::new(0));
        let close_count = closed.clone();
        let raw = Rc::new(Cell::new(0isize));
        let observed_raw = raw.clone();
        let requested_mode = mode.clone();
        Application::new().run(move |cx| {
            cx.on_window_closed(move |cx| {
                close_count.set(close_count.get() + 1);
                let mut app = cx.to_async();
                cx.foreground_executor()
                    .spawn(async move {
                        Timer::after(Duration::from_millis(100)).await;
                        app.update(|cx| cx.quit()).expect("quit after drain");
                    })
                    .detach();
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(500.), px(400.)), cx);
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    move |window, cx| {
                        let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window)
                            .expect("native window handle")
                            .as_raw()
                        else {
                            panic!("expected Win32 window");
                        };
                        raw.set(handle.hwnd.get());
                        cx.new(|_| Probe)
                    },
                )
                .expect("open real GPUI window");
            let hwnd = observed_raw.get();
            assert_ne!(hwnd, 0, "the fixture must create a real native HWND");
            let mut app = cx.to_async();
            cx.foreground_executor()
                .spawn(async move {
                    Timer::after(Duration::from_millis(250)).await;
                    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
                    // SAFETY: this task runs on the HWND's owning UI thread outside an App borrow.
                    unsafe {
                        assert!(
                            IsWindow(Some(hwnd)).as_bool(),
                            "window must be live before close"
                        );
                        match requested_mode.as_str() {
                            "queued-active-close" => {
                                SendMessageW(hwnd, WM_ACTIVATE, Some(WPARAM(0)), Some(LPARAM(0)));
                                SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
                            }
                            "native-close" => {
                                SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
                            }
                            _ => {
                                app.update_window(handle.into(), |_, window, _| {
                                    window.remove_window()
                                })
                                .expect("remove live window through App");
                            }
                        }
                    }
                })
                .detach();
            cx.activate(true);
        });
        assert_eq!(
            closed.get(),
            1,
            "{mode} must emit exactly one close callback"
        );
        // SAFETY: querying the captured numeric handle does not dereference the former window.
        assert!(
            !unsafe { IsWindow(Some(HWND(observed_raw.get() as *mut std::ffi::c_void))) }.as_bool(),
            "{mode} must destroy the original native HWND"
        );
        let errors = ERRORS.lock().expect("inspect captured native errors");
        assert!(
            errors.is_empty(),
            "{mode} must close without native errors: {errors:#?}"
        );
        println!("PASS: {mode}; close_count=1; native_window_destroyed=true; errors=0");
    }
}

fn main() {
    #[cfg(windows)]
    native::run();
    #[cfg(not(windows))]
    panic!("the native lifecycle probe requires Windows");
}

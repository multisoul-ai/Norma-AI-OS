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
                let message = format!(
                    "{}:{} {}: {}",
                    record.file().unwrap_or("unknown"),
                    record.line().unwrap_or(0),
                    record.target(),
                    record.args()
                );
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

    /// 场景：显式quit结束消息循环后，退出观察器和实体析构排队的前台收尾仍须完成。
    /// 构造：真实顶层HWND、独立退出观察器标志和观察器排队的前台marker。
    /// 执行：窗口存活250毫秒后直接cx.quit；Application::run返回后查询marker和IsWindow。
    /// 预期：观察器及marker均执行，原HWND已销毁；自唤醒工作须有界收口并明确报超时。
    fn explicit_quit(self_waking: bool) {
        let raw = Rc::new(Cell::new(0isize));
        let observed = Rc::new(Cell::new(false));
        let finished = Rc::new(Cell::new(false));
        let captured = raw.clone();
        let quit_observed = observed.clone();
        let drained = finished.clone();
        Application::new().run(move |cx| {
            cx.on_app_quit(move |cx| {
                quit_observed.set(true);
                let drained = drained.clone();
                cx.foreground_executor()
                    .spawn(async move {
                        drained.set(true);
                        println!("explicit-quit foreground marker completed");
                    })
                    .detach();
                if self_waking {
                    cx.foreground_executor()
                        .spawn(std::future::poll_fn(|cx| {
                            cx.waker().wake_by_ref();
                            std::task::Poll::<()>::Pending
                        }))
                        .detach();
                }
                println!("explicit-quit observer completed; self_waking={self_waking}");
                async {}
            })
            .detach();
            cx.open_window(WindowOptions::default(), move |window, cx| {
                let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window)
                    .expect("explicit quit native handle")
                    .as_raw()
                else {
                    panic!("expected explicit quit HWND")
                };
                captured.set(handle.hwnd.get());
                cx.new(|_| Probe)
            })
            .expect("open actual window before explicit quit");
            let mut app = cx.to_async();
            cx.foreground_executor()
                .spawn(async move {
                    Timer::after(Duration::from_millis(250)).await;
                    println!("requesting explicit quit; self_waking={self_waking}");
                    app.update(|cx| {
                        assert_eq!(cx.windows().len(), 1, "显式退出时必须仍有一个真实窗口");
                        cx.quit();
                    })
                    .expect("request quit while the native window is live");
                })
                .detach();
        });
        assert_ne!(raw.get(), 0, "必须实际创建顶层HWND，不能由无窗口样本假阳性");
        let alive = unsafe { IsWindow(Some(HWND(raw.get() as *mut std::ffi::c_void))) }.as_bool();
        println!(
            "explicit-quit: observer={}; foreground_marker={}; native_window_alive={alive}",
            observed.get(),
            finished.get()
        );
        assert!(observed.get(), "退出观察器必须确实执行");
        assert!(
            finished.get(),
            "退出观察器排队的前台收尾必须完成，实际HWND仍活={alive}"
        );
        assert!(
            !alive,
            "Application::run返回前必须销毁显式退出遗留的顶层HWND"
        );
        let errors = ERRORS.lock().expect("inspect explicit quit native errors");
        if self_waking {
            assert_eq!(
                errors.len(),
                1,
                "必须明确报告无法收敛的前台工作，不能静默丢弃：{errors:#?}"
            );
            assert!(
                errors[0].contains("native foreground cleanup did not quiesce"),
                "超时诊断必须准确指出不收敛的前台工作：{errors:#?}"
            );
        } else {
            assert!(errors.is_empty(), "显式退出不能产生native错误：{errors:#?}");
        }
        println!("PASS: explicit-quit; quit_observed=true; drain_finished=true; all_native_windows_destroyed=true; self_waking={self_waking}; errors={}", errors.len());
    }

    /// 场景：窗口完整收口；App选择后台保活时，关闭最后窗口不能退出消息循环。
    /// 数据构造：实际500×400目标窗；既有四模式带隐藏keeper，后台模式完全没有keeper。
    /// 执行过程：250毫秒后关窗，100毫秒后记录排空；后台模式此时才显式quit。
    /// 预期结果：只关闭一次且HWND销毁；后台模式必须执行无窗口期间的UI任务，无native错误。
    pub(super) fn run() {
        let mode = std::env::args().nth(1).expect("lifecycle mode required");
        assert!(
            [
                "native-close",
                "queued-active-close",
                "queued-window-ops-close",
                "remove-window",
                "background-last-close",
                "explicit-quit",
                "self-waking-quit"
            ]
            .contains(&mode.as_str()),
            "unknown lifecycle mode: {mode}"
        );
        let background = mode == "background-last-close";
        log::set_logger(&Capture).expect("install native error capture");
        log::set_max_level(log::LevelFilter::Error);
        if mode == "explicit-quit" || mode == "self-waking-quit" {
            explicit_quit(mode == "self-waking-quit");
            return;
        }
        let closed = Rc::new(Cell::new(0));
        let close_count = closed.clone();
        let raw = Rc::new(Cell::new(0isize));
        let observed_raw = raw.clone();
        let retired_raw = raw.clone();
        let keeper_raw = Rc::new(Cell::new(0isize));
        let retired_keeper = keeper_raw.clone();
        let drained = Rc::new(Cell::new(false));
        let finished = drained.clone();
        let requested_mode = mode.clone();
        Application::new().run(move |cx| {
            if background {
                cx.set_quit_on_last_window_close(false);
            }
            let keeper = (!background).then(|| {
                cx.open_window(
                    WindowOptions {
                        show: false,
                        ..Default::default()
                    },
                    move |window, cx| {
                        let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window)
                            .expect("keeper native handle")
                            .as_raw()
                        else {
                            panic!("expected keeper HWND")
                        };
                        keeper_raw.set(handle.hwnd.get());
                        cx.new(|_| Probe)
                    },
                )
                .expect("open hidden keeper to prevent early native quit")
            });
            cx.on_window_closed(move |cx| {
                if cx.windows().len() != usize::from(!background) {
                    return;
                }
                close_count.set(close_count.get() + 1);
                let mut app = cx.to_async();
                let finished = finished.clone();
                cx.foreground_executor()
                    .spawn(async move {
                        Timer::after(Duration::from_millis(100)).await;
                        finished.set(true);
                        if let Some(keeper) = keeper {
                            app.update_window(keeper.into(), |_, window, _| window.remove_window())
                                .expect("remove keeper after verified drain");
                        } else {
                            app.update(|cx| {
                                assert!(
                                    cx.windows().is_empty(),
                                    "background task must execute with no windows"
                                );
                                cx.quit();
                            })
                            .expect("explicitly quit after the background task executed");
                        }
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
                    if requested_mode == "queued-window-ops-close" {
                        app.update_window(handle.into(), |_, window, _| {
                            window.activate_window();
                            window.resize(size(px(450.), px(350.)));
                            window.toggle_fullscreen();
                        })
                        .expect("queue operations while the target is still logically live");
                    }
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
                            "native-close"
                            | "queued-window-ops-close"
                            | "background-last-close" => {
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
        assert!(
            drained.get(),
            "{mode} must finish the post-close drain before quitting"
        );
        // SAFETY: querying the captured numeric handle does not dereference the former window.
        assert!(
            !unsafe { IsWindow(Some(HWND(retired_raw.get() as *mut std::ffi::c_void))) }.as_bool(),
            "{mode} must destroy the original native HWND"
        );
        if background {
            assert_eq!(
                retired_keeper.get(),
                0,
                "background mode must never create a keeper window"
            );
        } else {
            assert_ne!(
                retired_keeper.get(),
                0,
                "the keeper must have a real native HWND"
            );
            assert!(
                !unsafe { IsWindow(Some(HWND(retired_keeper.get() as *mut std::ffi::c_void))) }
                    .as_bool(),
                "{mode} must also destroy the keeper HWND"
            );
        }
        let errors = ERRORS.lock().expect("inspect captured native errors");
        assert!(
            errors.is_empty(),
            "{mode} must close without native errors: {errors:#?}"
        );
        println!("PASS: {mode}; close_count=1; drain_finished=true; all_native_windows_destroyed=true; keeper_present={}; errors=0", !background);
    }
}

fn main() {
    #[cfg(windows)]
    native::run();
    #[cfg(not(windows))]
    panic!("the native lifecycle probe requires Windows");
}

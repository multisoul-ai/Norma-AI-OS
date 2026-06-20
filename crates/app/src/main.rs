//! multisoul-desktop 二进制入口 — Task 2 spike：验证 gpui 可构建 + 开空白窗口。
use gpui::{
    div, prelude::*, px, size, App, Application, Bounds, Context, Window,
    WindowBounds, WindowOptions,
};

struct RootView;

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(gpui::rgb(0x1e1e1e))
            .child("multisoul-desktop spike: blank canvas")
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|_cx| RootView),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}

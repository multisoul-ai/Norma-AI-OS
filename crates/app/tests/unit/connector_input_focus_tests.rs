use super::*;
use gpui::{size, FocusHandle, Point, TestAppContext};

/// Mimics the settings panel: bubble-phase mouse down refocuses the panel.
struct Panel {
    focus: FocusHandle,
    input: Entity<InputState>,
}

impl Render for Panel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus.focus(window);
                cx.stop_propagation();
            })
            .child(
                focus_guard("connector-port-input", &self.input, false, cx)
                    .debug_selector(|| "connector-port-input".into())
                    .w(px(140.0))
                    .child(Input::new(&self.input)),
            )
    }
}

/// Clicking the connector port field inside a self-refocusing panel must leave the
/// Input focused (regression: typing was impossible because the panel stole focus).
#[test]
fn clicking_connector_input_keeps_input_focus_inside_settings_panel() {
    let mut test = TestAppContext::single();
    test.update(|cx| {
        ui::sync_component_theme(
            ui::ThemeId::ALL
                .into_iter()
                .find(|id| !id.is_dark())
                .unwrap(),
            cx,
        )
    });
    let mut input_entity = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let input = cx.new(|cx| InputState::new(window, cx));
        input_entity = Some(input.clone());
        let view = cx.new(|cx| Panel {
            focus: cx.focus_handle(),
            input,
        });
        ui::component::Root::new(view, window, cx)
    });
    let input = input_entity.unwrap();
    cx.simulate_resize(size(px(400.0), px(200.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let bounds = cx
        .debug_bounds("connector-port-input")
        .expect("input should be drawn");
    let point: Point<_> = bounds.center();
    cx.simulate_click(point, gpui::Modifiers::none());
    cx.run_until_parked();
    cx.simulate_input("4");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            input.read(cx).focus_handle(cx).is_focused(window),
            "panel mouse-down must not steal focus from the connector input"
        );
        assert_eq!(input.read(cx).value().as_ref(), "4");
    });
}

/// 场景：连接器设置渲染复制Funnel命令入口，端口修改后复制当前已保存值。
/// 构造：真实Controller初始47615，通过GPUI update改为48000；窗口800×1000容纳控件。
/// 执行：绘制、点击复制、读取Clipboard，再改端口、重绘并复制。
/// 预期：两次分别复制47615和48000；渲染不能固化初始端口。
#[test]
fn connector_settings_copies_funnel_command_for_saved_port() {
    let mut test = TestAppContext::single();
    test.update(|cx| {
        ui::sync_component_theme(
            ui::ThemeId::ALL
                .into_iter()
                .find(|id| !id.is_dark())
                .expect("浅色"),
            cx,
        )
    });
    let mut controller = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let (commands, _) = async_channel::unbounded();
        let view = cx.new(|_| Controller {
            commands,
            config: Default::default(),
            listening: false,
            busy: false,
            error: None,
            expiries: Vec::new(),
            locked_until: 0,
            audit: Vec::new(),
            port: None,
            tunnel: None,
            theme: Theme::default(),
            updates: None,
            capture: None,
            signer: None,
        });
        controller = Some(view.clone());
        ui::component::Root::new(view, window, cx)
    });
    let controller = controller.expect("Controller");
    cx.simulate_resize(size(px(800.0), px(1000.0)));
    for (port, expected) in [
        (47615, "tailscale funnel --bg 47615"),
        (48000, "tailscale funnel --bg 48000"),
    ] {
        cx.update(|_, cx| {
            controller.update(cx, |host, cx| {
                host.config.port = port;
                cx.notify();
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let bounds = cx
            .debug_bounds("connector-funnel-copy")
            .expect("复制入口必须渲染");
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some(expected.into()),
                "复制已保存端口{port}对应的命令"
            );
        });
    }
}

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

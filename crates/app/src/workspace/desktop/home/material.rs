use gpui::{div, linear_color_stop, linear_gradient, prelude::*, px, rgba, BoxShadow, Div};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::icons::with_alpha;

pub(in crate::workspace::desktop) fn progress(
    key: impl Into<gpui::ElementId>,
    target: bool,
    milliseconds: u64,
    reduced: bool,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> f32 {
    let state = window.use_keyed_state(key, cx, |_, _| super::motion::HomeTween::default());
    let now = std::time::Instant::now();
    state.update(cx, |tween, _| {
        tween.target(
            target,
            now,
            std::time::Duration::from_millis(milliseconds),
            reduced,
        );
        if tween.active(now) {
            window.request_animation_frame();
        }
        tween.value(now)
    })
}

pub(crate) fn home_theme(id: ui::ThemeId) -> ui::Theme {
    let mut theme = crate::app::top_bar_theme(ui::Theme::for_id(id));
    theme.panel = 0x10161ea8;
    theme.panel_high = rgba(theme.panel).blend(rgba(0xe3e3e814)).into();
    theme.node_fill = rgba(theme.panel).blend(rgba(0xe3e3e81f)).into();
    theme.canvas_to = 0x090d13ff;
    theme.text_primary = 0xf1f3f8ff;
    theme.text_secondary = 0xe8ebf0ff;
    theme.text_tertiary = 0xe2e6ecff;
    theme.node_stroke = 0xffffff1f;
    theme.focused_stroke = 0xa8c7faff;
    theme.accent = 0xa8c7faff;
    theme.success = 0x6dd58cff;
    theme.destructive = 0xf2b8b5ff;
    theme
}

pub(crate) fn ring(color: u32) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(color).into(),
        offset: gpui::point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(3.0),
    }]
}

pub(crate) fn secondary(theme: ui::Theme, cx: &gpui::App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(rgba(0x00000000).into())
        .foreground(rgba(theme.text_primary).into())
        .border(rgba(0x00000000).into())
        .hover(rgba(with_alpha(theme.text_primary, 20)).into())
        .active(rgba(with_alpha(theme.text_primary, 31)).into())
}

pub(crate) fn button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: ui::Theme,
    cx: &gpui::App,
) -> Button {
    Button::new(id)
        .custom(secondary(theme, cx))
        .rounded(px(18.0))
        .border_3()
        .border_color(rgba(0x00000000))
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_primary))
                .child(label.into()),
        )
        .focus(move |s| {
            s.bg(rgba(with_alpha(theme.text_primary, 31)))
                .border_color(rgba(theme.focused_stroke))
        })
}

pub(crate) fn scrim(top: bool, height: f32) -> Div {
    let (from, to) = if top {
        (0x02060c2e, 0x02060c00)
    } else {
        (0x02060c00, 0x02060c38)
    };
    div()
        .absolute()
        .left_0()
        .right_0()
        .h(px(height))
        .when(top, |s| s.top_0())
        .when(!top, |s| s.bottom_0())
        .bg(linear_gradient(
            180.0,
            linear_color_stop(rgba(from), 0.0),
            linear_color_stop(rgba(to), 1.0),
        ))
}

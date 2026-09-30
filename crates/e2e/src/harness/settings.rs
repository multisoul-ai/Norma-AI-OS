use gpui::{point, px, ScrollDelta, ScrollWheelEvent, VisualTestContext};

pub(crate) fn scroll_settings_target_into_view(cx: &mut VisualTestContext, selector: &'static str) {
    // 8 次足以覆盖当前四张模型卡；24px 保留点击点与裁剪边缘的安全距离。
    for _ in 0..8 {
        cx.run_until_parked();
        let Some(content) = cx.debug_bounds("settings-content-scroll") else {
            return;
        };
        let Some(target) = cx.debug_bounds(selector) else {
            return;
        };
        let margin_px = 24.0;
        let visible_top = f32::from(content.origin.y) + margin_px;
        let visible_bottom =
            f32::from(content.origin.y) + f32::from(content.size.height) - margin_px;
        let target_top = f32::from(target.origin.y);
        let target_bottom = target_top + f32::from(target.size.height);
        let delta_y = if target_top < visible_top {
            visible_top - target_top
        } else if target_bottom > visible_bottom {
            visible_bottom - target_bottom
        } else {
            return;
        };
        cx.simulate_event(ScrollWheelEvent {
            position: content.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(delta_y))),
            ..Default::default()
        });
    }
}

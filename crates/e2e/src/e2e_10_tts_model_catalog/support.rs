use app::MultisoulApp;
use gpui::{point, px, Modifiers, ScrollDelta, ScrollWheelEvent, VisualTestContext};

// 每次用户操作最多派生 4 层任务；8 次收口留出两轮余量。
const E2E_SETTLE_PASSES: usize = 8;
// 3 次校正覆盖上下方与重排补偿；24px 避免目标贴住裁剪边缘。
const SETTINGS_SCROLL_ATTEMPTS: usize = 3;
const SETTINGS_SCROLL_MARGIN_PX: f32 = 24.0;

pub(super) fn snapshot(
    app: &gpui::Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
) -> app::SettingsE2eSnapshot {
    app.update(cx, |app, cx| app.settings_e2e_snapshot(cx))
}

pub(super) fn settle(cx: &mut VisualTestContext) {
    for _ in 0..E2E_SETTLE_PASSES {
        cx.run_until_parked();
    }
}

pub(super) fn click(
    app: &gpui::Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
    selector: &'static str,
) {
    cx.run_until_parked();
    scroll_settings_target_into_view(app, cx, selector);
    app.update(cx, |_app, cx| cx.notify());
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} 应可点击"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

pub(super) fn write_pcm_wav(path: &std::path::Path, sample_rate: u32, channels: u16, seconds: u32) {
    let sample_count = sample_rate * seconds * u32::from(channels);
    let data_bytes = sample_count * 2;
    let mut bytes = Vec::with_capacity((44 + data_bytes) as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    bytes.resize((44 + data_bytes) as usize, 0);
    std::fs::write(path, bytes).expect("e2e 参考 WAV 应写入临时目录");
}

fn scroll_settings_target_into_view(
    app: &gpui::Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
    selector: &'static str,
) {
    for _ in 0..SETTINGS_SCROLL_ATTEMPTS {
        app.update(cx, |_app, cx| cx.notify());
        cx.run_until_parked();
        let Some(content) = cx.debug_bounds("settings-content-scroll") else {
            return;
        };
        let Some(target) = cx.debug_bounds(selector) else {
            return;
        };
        let content_left = f32::from(content.origin.x);
        let content_right = content_left + f32::from(content.size.width);
        let target_center_x = f32::from(target.center().x);
        if !(content_left..=content_right).contains(&target_center_x) {
            return;
        }
        let margin = SETTINGS_SCROLL_MARGIN_PX;
        let visible_top = f32::from(content.origin.y) + margin;
        let visible_bottom = f32::from(content.origin.y) + f32::from(content.size.height) - margin;
        let target_top = f32::from(target.origin.y);
        let target_bottom = target_top + f32::from(target.size.height);
        let delta = if target_top < visible_top {
            visible_top - target_top
        } else if target_bottom > visible_bottom {
            visible_bottom - target_bottom
        } else {
            return;
        };
        cx.simulate_event(ScrollWheelEvent {
            position: content.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(delta))),
            ..Default::default()
        });
        cx.run_until_parked();
    }
}

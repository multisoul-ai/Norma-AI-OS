//! 把 Command Dock 的窗口坐标告诉语音光晕：声纹、星尘都从 Dock 位置出图。

use crate::app::MultisoulApp;
use crate::effects::voice_glow::scene::Rect;
use gpui::{canvas, prelude::*, Canvas};

impl MultisoulApp {
    pub(crate) fn voice_glow_dock_probe(&self) -> Canvas<()> {
        let bus = self.voice_window_inset_events.clone();
        canvas(
            move |bounds, _, _| {
                bus.set_dock(Rect {
                    x: f32::from(bounds.origin.x),
                    y: f32::from(bounds.origin.y),
                    w: f32::from(bounds.size.width),
                    h: f32::from(bounds.size.height),
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }
}

//! 运行概览悬浮面板。
//!
//! 设计注记：
//! - 参考 apple-design、emil-design-eng。
//! - 采纳：拖动 1:1 跟手，过程中没有过渡；松手后停在限位位置，不追加弹簧或动量。
//!   规格要求面板留在画布内，边界是硬停，不是橡皮筋。
//! - 采纳：弹出是偶尔发生的出现。180ms ease-out 只淡入。减少动态效果时仍只淡入，
//!   时长 120ms，没有位移。拖动一开始就把淡入收到不透明，避免拖动中还在过渡。
//! - 拒绝入场缩放。GPUI 0.2.2 不能给 div 加 transform，规格写的也是淡入。
//! - 减少透明度时面板用实色；否则用原型的半透明玻璃色。圆角画在带背景的同一层上，
//!   子层不铺满不透明底，因为 overflow_hidden 只做矩形裁剪。
//! - 图钉和移动手柄用 ui::component::Button。悬停、按下、焦点即时变色，尺寸不变。
//!   悬浮中的图钉是按下态。手柄没有禁用态：它只在面板悬浮时出现。

use crate::motion::scalar::AnimatedScalar;
use crate::motion::util::{ease_out_cubic, lerp, EPSILON};
use runtime::desktop_drawers::OverviewOffset;
use std::time::{Duration, Instant};

const FADE_MS: u64 = 180;
const REDUCED_FADE_MS: u64 = 120;
const DRAG_NOTIFY_INTERVAL: Duration = Duration::from_millis(16);
const PANEL_RADIUS: f32 = 14.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct OverviewFloatFade {
    from_opacity: f32,
    showing: bool,
    progress: AnimatedScalar,
}

impl Default for OverviewFloatFade {
    fn default() -> Self {
        Self::hidden()
    }
}

impl OverviewFloatFade {
    pub(crate) fn hidden() -> Self {
        Self {
            from_opacity: 0.0,
            showing: false,
            progress: AnimatedScalar::on(),
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        (self.progress.value - 1.0).abs() > EPSILON
    }

    pub(crate) fn opacity(&self) -> f32 {
        let target = if self.showing { 1.0 } else { 0.0 };
        lerp(
            self.from_opacity,
            target,
            ease_out_cubic(self.progress.value),
        )
    }

    pub(crate) fn retarget(&mut self, showing: bool) {
        if showing == self.showing && !self.is_active() {
            return;
        }
        self.from_opacity = self.opacity();
        self.showing = showing;
        self.progress.value = 0.0;
        self.progress.target = 1.0;
    }

    /// Dragging must not keep the entrance fade running.
    pub(crate) fn snap_visible(&mut self) {
        self.from_opacity = 1.0;
        self.showing = true;
        self.progress = AnimatedScalar::on();
    }

    pub(crate) fn tick(&mut self, delta: Duration, reduce_motion: bool) -> bool {
        let duration = Duration::from_millis(if reduce_motion {
            REDUCED_FADE_MS
        } else {
            FADE_MS
        });
        self.progress.tick(delta, duration)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct OverviewDrag {
    origin_x: f32,
    origin_y: f32,
    origin_offset: OverviewOffset,
    last_notify: Option<Instant>,
}

pub(crate) fn overview_panel_fill(theme: ui::Theme, reduce_transparency: bool) -> u32 {
    ui::icons::with_alpha(
        theme.canvas_from,
        if reduce_transparency { 0xff } else { 0xf5 },
    )
}

fn prefers_reduced_transparency() -> bool {
    match std::env::var("MULTISOUL_REDUCE_TRANSPARENCY") {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => system_reduce_transparency(),
    }
}

fn system_reduce_transparency() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        unsafe {
            let Some(class) = AnyClass::get(c"NSWorkspace") else {
                return false;
            };
            let workspace: *mut AnyObject = msg_send![class, sharedWorkspace];
            if workspace.is_null() {
                return false;
            }
            let reduced: bool = msg_send![workspace, accessibilityDisplayShouldReduceTransparency];
            reduced
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

mod drag;
mod panel;

#[cfg(test)]
mod tests {
    use super::*;

    /// 减少透明度时悬浮面板改用实色，普通状态保留半透明玻璃。
    ///
    /// 数据构造：两种系统偏好的布尔值。
    /// 执行过程：分别取填充色。
    /// 预期结果：实色 alpha 为 0xff；玻璃色 alpha 不是 0xff，且两者不相等。
    #[test]
    fn reduced_transparency_uses_an_opaque_panel_fill() {
        let solid = overview_panel_fill(ui::Theme::paper_blue(), true);
        let glass = overview_panel_fill(ui::Theme::paper_blue(), false);
        assert_eq!(solid & 0xff, 0xff, "减少透明度时背景应为实色");
        assert_ne!(glass & 0xff, 0xff, "默认玻璃背景应保留透明通道");
        assert_ne!(solid, glass, "两种偏好不能画成同一种填充");
    }

    /// 弹出只淡入；减少动态效果时同样只有透明度，没有位移。
    ///
    /// 数据构造：隐藏状态开始，目标改为显示。时长 180ms，减少动态效果为 120ms。
    /// 执行过程：读起始不透明度，推进一个完整时长。
    /// 预期结果：从 0 淡到 1；结构里没有偏移字段可被这次动画改写。
    #[test]
    fn float_entrance_fades_without_moving() {
        let mut fade = OverviewFloatFade::hidden();
        fade.retarget(true);
        assert!(
            fade.opacity() < 0.05,
            "淡入应从接近透明开始，而不是已经不透明"
        );
        assert!(
            fade.tick(Duration::from_millis(FADE_MS), false),
            "180ms 内淡入应推进"
        );
        assert!((fade.opacity() - 1.0).abs() < 0.02, "180ms 后应淡到不透明");
        fade.snap_visible();
        assert!(
            (fade.opacity() - 1.0).abs() < EPSILON,
            "拖动开始后应立刻不透明，不再过渡"
        );
        let mut reduced = OverviewFloatFade::hidden();
        reduced.retarget(true);
        assert!(
            reduced.tick(Duration::from_millis(REDUCED_FADE_MS), true),
            "减少动态效果时 120ms 淡入应完成"
        );
        assert!(
            (reduced.opacity() - 1.0).abs() < 0.02,
            "减少动态效果时结束态仍是不透明，没有别的运动量"
        );
    }
}

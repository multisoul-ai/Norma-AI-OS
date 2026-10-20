//! Directional slide between the chat and run-overview views.
//!
//! 设计注记见 `left_tabs.rs`。这里只实现可中断的呈现值：
//! 新进入概览从 +10px 开始，回到对话从 -10px 开始，都滑到 0 并淡入。
//! 位移由绝对定位落地，因为 GPUI 0.2.2 不能给 div 加 transform。
//! 减少动态效果时位移保持 0。中途反向从当前呈现位置继续，不跳回入口。

use crate::motion::scalar::AnimatedScalar;
use crate::motion::util::{ease_out_cubic, lerp};
use std::time::Duration;

const TRAVEL_PX: f32 = 10.0;
const MOVE_MS: u64 = 200;
const FADE_MS: u64 = 120;

#[derive(Clone, Copy, Debug)]
pub(crate) struct LeftViewSlide {
    from_x: f32,
    to_x: f32,
    from_opacity: f32,
    progress: AnimatedScalar,
}

impl Default for LeftViewSlide {
    fn default() -> Self {
        Self::settled()
    }
}

impl LeftViewSlide {
    pub(crate) fn settled() -> Self {
        Self {
            from_x: 0.0,
            to_x: 0.0,
            from_opacity: 1.0,
            progress: AnimatedScalar::on(),
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        (self.progress.value - 1.0).abs() > crate::motion::util::EPSILON
    }

    pub(crate) fn retarget(&mut self, toward_overview: bool, reduce_motion: bool) {
        let current_x = self.offset(reduce_motion);
        let current_opacity = self.opacity();
        let fresh = !self.is_active();
        self.from_x = if fresh && !reduce_motion {
            if toward_overview {
                TRAVEL_PX
            } else {
                -TRAVEL_PX
            }
        } else if reduce_motion {
            0.0
        } else {
            current_x
        };
        self.to_x = 0.0;
        self.from_opacity = if fresh { 0.0 } else { current_opacity };
        self.progress.value = 0.0;
        self.progress.target = 1.0;
    }

    pub(crate) fn offset(&self, reduce_motion: bool) -> f32 {
        if reduce_motion {
            0.0
        } else {
            lerp(self.from_x, self.to_x, ease_out_cubic(self.progress.value))
        }
    }

    pub(crate) fn opacity(&self) -> f32 {
        lerp(self.from_opacity, 1.0, ease_out_cubic(self.progress.value))
    }

    pub(crate) fn tick(&mut self, delta: Duration, reduce_motion: bool) -> bool {
        let duration = Duration::from_millis(if reduce_motion { FADE_MS } else { MOVE_MS });
        self.progress.tick(delta, duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 进入运行概览从右侧 10px 淡入，200ms 后停在原位且完全可见。
    ///
    /// 数据构造：已静止的切换；10px 为规格位移，200ms 为规格时长。
    /// 执行过程：朝概览重定向，读取起点，再推进正好 200ms。
    /// 预期结果：起点 x=10、透明度 0；结束 x=0、透明度 1，且不是从左侧进入。
    #[test]
    fn overview_enters_from_the_right_and_settles_in_200ms() {
        let mut slide = LeftViewSlide::settled();
        slide.retarget(true, false);
        assert!(
            (slide.offset(false) - 10.0).abs() < 0.01,
            "进入概览的起点应在右侧 10px"
        );
        assert!(slide.opacity() < 0.01, "进入概览的起点应完全透明");
        assert!(
            slide.tick(Duration::from_millis(200), false),
            "200ms 应推进动画"
        );
        assert!(slide.offset(false).abs() < 0.01, "200ms 后应停在内容原位");
        assert!((slide.opacity() - 1.0).abs() < 0.01, "200ms 后应完全可见");
        assert!(slide.offset(false) > -1.0, "进入概览不应从左侧滑入");
    }

    /// 减少动态效果时只淡入，位移保持 0，120ms 后可见。
    ///
    /// 数据构造：减少动态效果开启；120ms 为规格淡入时长。
    /// 执行过程：朝对话重定向并读取起点，再推进 120ms。
    /// 预期结果：起点和结束的位移都是 0；起点透明，120ms 后不透明。
    #[test]
    fn reduced_motion_fades_without_sliding() {
        let mut slide = LeftViewSlide::settled();
        slide.retarget(false, true);
        assert!(
            slide.offset(true).abs() < 0.01,
            "减少动态效果时起点不应有水平位移"
        );
        assert!(slide.opacity() < 0.01, "减少动态效果时仍从透明淡入");
        assert!(
            slide.tick(Duration::from_millis(120), true),
            "120ms 应完成淡入"
        );
        assert!((slide.opacity() - 1.0).abs() < 0.01, "120ms 后应完全可见");
        assert!(
            slide.offset(true).abs() < 0.01,
            "减少动态效果结束时仍不应有位移"
        );
    }

    /// 动画进行到一半时反向，应从当前呈现位置继续，而不是跳到另一侧入口。
    ///
    /// 数据构造：先进入概览并推进 100ms（200ms 的一半）。
    /// 执行过程：再朝对话重定向，立刻读取新的起点。
    /// 预期结果：新起点仍大于 0，不是对话入口的 -10px。
    #[test]
    fn reversal_continues_from_the_presented_offset() {
        let mut slide = LeftViewSlide::settled();
        slide.retarget(true, false);
        assert!(
            slide.tick(Duration::from_millis(100), false),
            "半程应仍在移动"
        );
        let presented = slide.offset(false);
        slide.retarget(false, false);
        let restarted = slide.offset(false);
        assert!(
            (restarted - presented).abs() < 0.01,
            "反向应从当前呈现位置 {presented} 开始，实际 {restarted}"
        );
        assert!(restarted > 0.0, "半程反向后不应跳到对话入口的 -10px");
        assert!(
            (restarted - (-10.0)).abs() > 1.0,
            "半程反向不得重置为左侧入口"
        );
    }
}

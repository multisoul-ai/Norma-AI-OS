//! Agent 启动看门狗：判断新建 Agent 在限值内是否收到过任何 hook 事件（GPUI-free 纯状态机）。
//!
//! 判据是 hook 而不是 PTY 输出：真实 Codex 联网失败时仍会绘制首屏，PTY 并不静默，
//! 但它不会发出 `SessionStart`。

use std::time::{Duration, Instant};

/// 新建后超过该时长仍未收到任何 hook 事件，则判定为「启动较慢」。
pub const STARTUP_SILENCE_LIMIT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug)]
pub struct StartupWatch {
    spawned_at: Instant,
    settled: bool,
    stalled: bool,
}

impl StartupWatch {
    pub fn new(now: Instant) -> Self {
        Self {
            spawned_at: now,
            settled: false,
            stalled: false,
        }
    }

    /// 收到该节点的 hook 事件：不再视为卡住，并撤销已有的 stalled。
    pub fn note_hook_seen(&mut self) {
        self.settled = true;
        self.stalled = false;
    }

    /// 返回 true 仅表示「本次刚越过阈值」，供调用方触发一次重绘。
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.settled || self.stalled {
            return false;
        }
        if now.saturating_duration_since(self.spawned_at) >= STARTUP_SILENCE_LIMIT {
            self.stalled = true;
            return true;
        }
        false
    }

    pub fn is_stalled(&self) -> bool {
        self.stalled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未满限值时不应判定卡住，恰好到限值时判定一次且只一次。
    ///
    /// 数据构造：
    ///   限值 STARTUP_SILENCE_LIMIT = 15s
    ///   t0 = 任意基准 Instant，spawned_at = t0
    ///   采样点：t0+14.9s（未满）、t0+15s（恰好满）、t0+30s（之后再次 tick）
    ///
    /// 执行过程：
    ///   1. new(t0)，从未 note_hook_seen → 没有 hook
    ///   2. tick(t0+14.9s) → 14.9 < 15，不触发
    ///   3. tick(t0+15s)   → 15 >= 15，首次越界，返回 true 并置 stalled
    ///   4. tick(t0+30s)   → 已 stalled，不重复触发
    ///
    /// 预期结果：
    ///   - 14.9s 时 tick 为 false 且 is_stalled 为 false
    ///   - 15s 时 tick 为 true 且 is_stalled 为 true
    ///   - 30s 时 tick 为 false（防止每帧重复要求重绘），但 is_stalled 仍为 true
    #[test]
    fn stalls_exactly_once_when_limit_reached_without_hook() {
        let t0 = Instant::now();
        let mut watch = StartupWatch::new(t0);

        let before = watch.tick(t0 + Duration::from_millis(14_900));
        assert!(!before, "14.9s 未到 15s 限值，不应触发 stalled");
        assert!(!watch.is_stalled(), "14.9s 时 is_stalled 必须为 false");

        let at_limit = watch.tick(t0 + STARTUP_SILENCE_LIMIT);
        assert!(
            at_limit,
            "恰好 15s 仍无 hook 应首次触发并返回 true 供 app 重绘"
        );
        assert!(watch.is_stalled(), "越过限值后 is_stalled 必须为 true");

        let again = watch.tick(t0 + Duration::from_secs(30));
        assert!(!again, "已 stalled 后再次 tick 不得重复返回 true");
        assert!(watch.is_stalled(), "没有 hook 时 stalled 必须保持");
    }

    /// 限值前已收到 hook 则永不判定卡住。
    ///
    /// 数据构造：t0 起，t0+3s 收到首个 hook（note_hook_seen），之后在 t0+60s tick。
    /// 推导：60s 远大于 15s 限值，若判据忽略 settled 则会误报。
    ///
    /// 执行过程：new(t0) → note_hook_seen() → tick(t0+60s)。
    ///
    /// 预期结果：tick 为 false，is_stalled 为 false（健康的 CLI 不得出现徽标）。
    #[test]
    fn never_stalls_once_hook_was_seen() {
        let t0 = Instant::now();
        let mut watch = StartupWatch::new(t0);

        watch.note_hook_seen();
        let fired = watch.tick(t0 + Duration::from_secs(60));

        assert!(!fired, "已收到 hook 的节点即使过了 60s 也不应触发 stalled");
        assert!(
            !watch.is_stalled(),
            "已收到 hook 的节点 is_stalled 必须为 false"
        );
    }

    /// 已 stalled 的节点收到 hook 后恢复，且不会再次误触发。
    ///
    /// 数据构造：t0 起无 hook 到 t0+15s 触发 stalled；之后 hook 到来；再在 t0+45s tick。
    ///
    /// 执行过程：
    ///   1. tick(t0+15s) → true，stalled
    ///   2. note_hook_seen() → 恢复
    ///   3. tick(t0+45s) → 已 settled
    ///
    /// 预期结果：note_hook_seen 后 is_stalled 为 false；之后 tick 为 false 且保持 false。
    #[test]
    fn hook_after_stall_clears_it_permanently() {
        let t0 = Instant::now();
        let mut watch = StartupWatch::new(t0);
        assert!(watch.tick(t0 + STARTUP_SILENCE_LIMIT), "前置：15s 应已触发");

        watch.note_hook_seen();

        assert!(!watch.is_stalled(), "hook 到来后徽标条件必须立即撤销");
        assert!(
            !watch.tick(t0 + Duration::from_secs(45)),
            "撤销后不得再次触发 stalled"
        );
        assert!(!watch.is_stalled(), "撤销后 is_stalled 必须保持 false");
    }
}

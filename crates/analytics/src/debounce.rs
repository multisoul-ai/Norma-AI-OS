//! `feature_used` 去抖：同一功能在窗口内只上报一次。

use std::collections::HashMap;
use std::hash::Hash;

pub const FEATURE_WINDOW_MS: u64 = 10_000;

pub struct Debouncer<K: Eq + Hash> {
    last: HashMap<K, u64>,
}

impl<K: Eq + Hash> Default for Debouncer<K> {
    fn default() -> Self {
        Self {
            last: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash> Debouncer<K> {
    /// 窗口内重复返回 `false`；窗口外或首次返回 `true` 并刷新时间。
    pub fn allow(&mut self, key: K, now_ms: u64, window_ms: u64) -> bool {
        match self.last.get(&key) {
            Some(prev) if now_ms.saturating_sub(*prev) < window_ms => false,
            _ => {
                self.last.insert(key, now_ms);
                true
            }
        }
    }
}

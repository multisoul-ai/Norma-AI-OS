//! 特效设置：缓存与纯逻辑在 `state.rs`，页面在后续文件。
//! 结构与壁纸页一致：范围 → 当前特效 → 特效库 → 按来源 → 播放。

mod current;
mod grid;
pub(crate) mod page;
mod playback;
mod preview;
mod scope;
pub(crate) mod sources;
pub(crate) mod state;
pub(crate) mod ui_state;
mod view;

pub(crate) use view::effects_settings;

#[cfg(test)]
#[path = "../../tests/unit/effects_settings_state_tests.rs"]
mod state_tests;

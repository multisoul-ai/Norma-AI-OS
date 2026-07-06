//! `app` crate 的库目标：暴露 GPUI shell（`MultisoulApp` / `AppScreen` / actions）
//! 给 e2e 测试 crate。二进制目标 `multisoul`（`src/main.rs`）隐式依赖本 lib。
//!
//! 模块声明保持私有（`mod`，非 `pub mod`）：内部模块树无需对外暴露，仅通过
//! 下方 `pub use` 释放 e2e / bin 所需的公开 API，避免泄漏 `pub(crate)` 类型。
mod app;
mod assets;
mod canvas;
mod cards;
mod frame_diagnostics;
mod motion;
mod norma;
mod preview;
mod settings;
mod shortcuts;
mod workspace;

pub use app::{
    AppScreen, CopyCardContent, ExitFocus, MultisoulApp, NewBrowser, NewClaudeAgent, NewCodexAgent,
    NewOpenCodeAgent, NewTerminal, NewWindow, OpenPreviewFile, OpenProject, ReflowLayout,
    ToggleMasterLayout, ToggleSettings,
};
pub use assets::EmbeddedAssets;

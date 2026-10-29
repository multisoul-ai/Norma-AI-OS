//! Embedded GPUI assets that should travel with the application binary.

use anyhow::Result;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

mod orb_frames;

pub const APP_ICON_ASSET: &str = "icons/app-icon.png";
pub const APP_LOGO_ASSET: &str = "icons/logo.png";
pub const HOME_BACKGROUND_ASSET: &str = "home/earth-horizon.png";
pub const DEFAULT_WALLPAPER_ASSET: &str = HOME_BACKGROUND_ASSET;
/// Toolbar SVG icon asset paths. These mirror `crate::ui::icons::ToolbarIcon::asset_path`
/// exactly (icons/toolbar-<name>.svg) so GPUI `svg().path(...)` can resolve them.
pub const TOOLBAR_SCROLL_ASSET: &str = "icons/toolbar-scroll.svg";
pub const TOOLBAR_LAYOUT_ASSET: &str = "icons/toolbar-layout.svg";
pub const TOOLBAR_PLUS_ASSET: &str = "icons/toolbar-plus.svg";
pub const TOOLBAR_TERMINAL_ASSET: &str = "icons/toolbar-terminal.svg";
pub const TOOLBAR_BROWSER_ASSET: &str = "icons/toolbar-browser.svg";
pub const TOOLBAR_CLAUDE_ASSET: &str = "icons/toolbar-claude.svg";
pub const TOOLBAR_CODEX_ASSET: &str = "icons/toolbar-codex.svg";
pub const TOOLBAR_CURSOR_ASSET: &str = "icons/toolbar-cursor.svg";
pub const TOOLBAR_GROK_ASSET: &str = "icons/toolbar-grok.svg";
pub const TOOLBAR_OPENCODE_ASSET: &str = "icons/toolbar-opencode.svg";
pub const TOOLBAR_INFCODE_ASSET: &str = "icons/toolbar-infcode.svg";
pub const TOOLBAR_KIMI_ASSET: &str = "icons/toolbar-kimi.svg";
pub const TOOLBAR_ZCODE_ASSET: &str = "icons/toolbar-zcode.svg";
pub const TOOLBAR_PI_ASSET: &str = "icons/toolbar-pi.svg";
pub const TOOLBAR_HERMES_ASSET: &str = "icons/toolbar-hermes.svg";
pub const TOOLBAR_PREVIEW_ASSET: &str = "icons/toolbar-preview.svg";
pub const TOOLBAR_SETTINGS_ASSET: &str = "icons/toolbar-settings.svg";
pub const TOOLBAR_WORKSPACES_ASSET: &str = "icons/toolbar-workspaces.svg";
pub const SETTINGS_NAV_THEMES_ASSET: &str = "icons/settings-nav-themes.svg";
pub const SETTINGS_NAV_WALLPAPER_ASSET: &str = "icons/settings-nav-wallpaper.svg";
pub const SETTINGS_NAV_KEYBOARD_ASSET: &str = "icons/settings-nav-keyboard.svg";
pub const SETTINGS_NAV_EFFECTS_ASSET: &str = "icons/settings-nav-effects.svg";
pub const SETTINGS_NAV_VOICE_ASSET: &str = "icons/settings-nav-voice.svg";
pub const SETTINGS_NAV_AGENTS_ASSET: &str = "icons/settings-nav-agents.svg";
pub const SETTINGS_NAV_HOOKS_ASSET: &str = "icons/settings-nav-hooks.svg";
pub const SETTINGS_CHECK_UPDATES_ASSET: &str = "icons/settings-check-updates.svg";
const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/icons/app-icon.png");
pub const NORMA_CALM_ASSET: &str = "icons/norma-calm.png";

const APP_LOGO_BYTES: &[u8] = include_bytes!("../assets/icons/logo.png");
const HOME_BACKGROUND_BYTES: &[u8] = include_bytes!("../assets/home/earth-horizon.png");
const TOOLBAR_SCROLL_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-scroll.svg");
const TOOLBAR_LAYOUT_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-layout.svg");
const TOOLBAR_PLUS_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-plus.svg");
const TOOLBAR_TERMINAL_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-terminal.svg");
const TOOLBAR_BROWSER_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-browser.svg");
const TOOLBAR_CLAUDE_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-claude.svg");
const TOOLBAR_CODEX_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-codex.svg");
const TOOLBAR_CURSOR_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-cursor.svg");
const TOOLBAR_GROK_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-grok.svg");
const TOOLBAR_OPENCODE_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-opencode.svg");
const TOOLBAR_INFCODE_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-infcode.svg");
const TOOLBAR_KIMI_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-kimi.svg");
const TOOLBAR_ZCODE_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-zcode.svg");
const TOOLBAR_PI_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-pi.svg");
const TOOLBAR_HERMES_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-hermes.svg");
const TOOLBAR_PREVIEW_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-preview.svg");
const TOOLBAR_SETTINGS_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-settings.svg");
const TOOLBAR_WORKSPACES_BYTES: &[u8] = include_bytes!("../assets/icons/toolbar-workspaces.svg");
const SETTINGS_NAV_THEMES_BYTES: &[u8] = include_bytes!("../assets/icons/settings-nav-themes.svg");
const SETTINGS_NAV_WALLPAPER_BYTES: &[u8] =
    include_bytes!("../assets/icons/settings-nav-wallpaper.svg");
const SETTINGS_NAV_EFFECTS_BYTES: &[u8] =
    include_bytes!("../assets/icons/settings-nav-effects.svg");
const SETTINGS_NAV_KEYBOARD_BYTES: &[u8] =
    include_bytes!("../assets/icons/settings-nav-keyboard.svg");
const SETTINGS_NAV_VOICE_BYTES: &[u8] = include_bytes!("../assets/icons/settings-nav-voice.svg");
const SETTINGS_NAV_AGENTS_BYTES: &[u8] = include_bytes!("../assets/icons/settings-nav-agents.svg");
const SETTINGS_NAV_HOOKS_BYTES: &[u8] = include_bytes!("../assets/icons/settings-nav-hooks.svg");
const SETTINGS_CHECK_UPDATES_BYTES: &[u8] =
    include_bytes!("../assets/icons/settings-check-updates.svg");
#[derive(Clone, Copy, Debug, Default)]
pub struct EmbeddedAssets;
impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(frame) = orb_frames::frame(path) {
            return Ok(Some(Cow::Borrowed(frame)));
        }
        match path {
            "icons/file-document.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/file-document.svg"
            )))),
            "icons/microphone.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/microphone.svg"
            )))),
            "icons/activity.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/activity.svg"
            )))),
            "icons/pin.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/pin.svg"
            )))),
            "icons/grip.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/grip.svg"
            )))),
            "icons/panel-left-close.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/panel-left-close.svg"
            )))),
            "icons/panel-left-open.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/panel-left-open.svg"
            )))),
            "icons/panel-right-close.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/panel-right-close.svg"
            )))),
            "icons/panel-right-open.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/panel-right-open.svg"
            )))),
            "icons/diff.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/diff.svg"
            )))),
            "icons/grid.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/grid.svg"
            )))),
            "icons/globe.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/globe.svg"
            )))),
            "icons/image.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/image.svg"
            )))),
            "icons/video.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/video.svg"
            )))),
            "icons/folder.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/folder.svg"
            )))),
            // gpui-component's searchable list/input use these standard icon paths.
            "icons/search.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/search.svg"
            )))),
            "icons/chevron-down.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/chevron-down.svg"
            )))),
            "icons/arrow-up.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/arrow-up.svg"
            )))),
            "icons/triangle-down.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/triangle-down.svg"
            )))),
            "icons/chevron-up.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/chevron-up.svg"
            )))),
            "icons/check.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/check.svg"
            )))),
            "icons/close.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/close.svg"
            )))),
            "icons/loader.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/loader.svg"
            )))),
            "icons/command-workspace.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/command-workspace.svg"
            )))),
            "icons/settings-nav-mobile.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/settings-nav-mobile.svg"
            )))),
            "icons/bell.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/bell.svg"
            )))),
            "icons/circle-x.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/circle-x.svg"
            )))),
            "icons/delete.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/delete.svg"
            )))),
            "icons/eye.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/eye.svg"
            )))),
            "icons/settings-2.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/settings-2.svg"
            )))),
            "icons/copy.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/copy.svg"
            )))),
            "icons/triangle-alert.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/triangle-alert.svg"
            )))),
            APP_ICON_ASSET => Ok(Some(Cow::Borrowed(APP_ICON_BYTES))),
            NORMA_CALM_ASSET => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/norma-calm.png"
            )))),
            APP_LOGO_ASSET => Ok(Some(Cow::Borrowed(APP_LOGO_BYTES))),
            HOME_BACKGROUND_ASSET => Ok(Some(Cow::Borrowed(HOME_BACKGROUND_BYTES))),
            TOOLBAR_SCROLL_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_SCROLL_BYTES))),
            TOOLBAR_LAYOUT_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_LAYOUT_BYTES))),
            TOOLBAR_PLUS_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_PLUS_BYTES))),
            TOOLBAR_TERMINAL_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_TERMINAL_BYTES))),
            TOOLBAR_BROWSER_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_BROWSER_BYTES))),
            TOOLBAR_CLAUDE_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CLAUDE_BYTES))),
            TOOLBAR_CODEX_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CODEX_BYTES))),
            TOOLBAR_CURSOR_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_CURSOR_BYTES))),
            TOOLBAR_GROK_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_GROK_BYTES))),
            TOOLBAR_OPENCODE_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_OPENCODE_BYTES))),
            TOOLBAR_INFCODE_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_INFCODE_BYTES))),
            TOOLBAR_KIMI_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_KIMI_BYTES))),
            TOOLBAR_ZCODE_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_ZCODE_BYTES))),
            TOOLBAR_PI_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_PI_BYTES))),
            TOOLBAR_HERMES_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_HERMES_BYTES))),
            TOOLBAR_PREVIEW_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_PREVIEW_BYTES))),
            TOOLBAR_SETTINGS_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_SETTINGS_BYTES))),
            TOOLBAR_WORKSPACES_ASSET => Ok(Some(Cow::Borrowed(TOOLBAR_WORKSPACES_BYTES))),
            SETTINGS_NAV_THEMES_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_THEMES_BYTES))),
            SETTINGS_NAV_WALLPAPER_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_WALLPAPER_BYTES))),
            SETTINGS_NAV_KEYBOARD_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_KEYBOARD_BYTES))),
            SETTINGS_NAV_EFFECTS_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_EFFECTS_BYTES))),
            SETTINGS_NAV_VOICE_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_VOICE_BYTES))),
            "icons/settings-nav-memory.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/settings-nav-memory.svg"
            )))),
            "icons/settings-nav-persona.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/settings-nav-persona.svg"
            )))),
            SETTINGS_NAV_AGENTS_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_AGENTS_BYTES))),
            SETTINGS_NAV_HOOKS_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_NAV_HOOKS_BYTES))),
            SETTINGS_CHECK_UPDATES_ASSET => Ok(Some(Cow::Borrowed(SETTINGS_CHECK_UPDATES_BYTES))),
            _ => Ok(None),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        match path {
            "" => Ok(vec![
                SharedString::from("icons"),
                SharedString::from("home"),
            ]),
            "home" => Ok(vec![SharedString::from("earth-horizon.png")]),
            "icons" => {
                let mut icons = vec![
                    SharedString::from("file-document.svg"),
                    SharedString::from("microphone.svg"),
                    SharedString::from("chevron-down.svg"),
                    SharedString::from("chevron-up.svg"),
                    SharedString::from("check.svg"),
                    SharedString::from("folder.svg"),
                    SharedString::from("diff.svg"),
                    SharedString::from("grid.svg"),
                    SharedString::from("globe.svg"),
                    SharedString::from("image.svg"),
                    SharedString::from("video.svg"),
                    SharedString::from("command-workspace.svg"),
                    SharedString::from("settings-nav-mobile.svg"),
                    SharedString::from("bell.svg"),
                    SharedString::from("circle-x.svg"),
                    SharedString::from("delete.svg"),
                    SharedString::from("eye.svg"),
                    SharedString::from("settings-2.svg"),
                    SharedString::from("copy.svg"),
                    SharedString::from("triangle-alert.svg"),
                    SharedString::from("app-icon.png"),
                    SharedString::from("logo.png"),
                    SharedString::from("norma-calm.png"),
                    SharedString::from("toolbar-scroll.svg"),
                    SharedString::from("toolbar-layout.svg"),
                    SharedString::from("toolbar-plus.svg"),
                    SharedString::from("toolbar-terminal.svg"),
                    SharedString::from("toolbar-browser.svg"),
                    SharedString::from("toolbar-claude.svg"),
                    SharedString::from("toolbar-codex.svg"),
                    SharedString::from("toolbar-cursor.svg"),
                    SharedString::from("toolbar-grok.svg"),
                    SharedString::from("toolbar-opencode.svg"),
                    SharedString::from("toolbar-infcode.svg"),
                    SharedString::from("toolbar-kimi.svg"),
                    SharedString::from("toolbar-zcode.svg"),
                    SharedString::from("toolbar-pi.svg"),
                    SharedString::from("toolbar-hermes.svg"),
                    SharedString::from("toolbar-preview.svg"),
                    SharedString::from("toolbar-settings.svg"),
                    SharedString::from("toolbar-workspaces.svg"),
                    SharedString::from("settings-nav-themes.svg"),
                    SharedString::from("settings-nav-wallpaper.svg"),
                    SharedString::from("settings-nav-keyboard.svg"),
                    SharedString::from("settings-nav-effects.svg"),
                    SharedString::from("settings-nav-voice.svg"),
                    SharedString::from("settings-nav-memory.svg"),
                    SharedString::from("settings-nav-persona.svg"),
                    SharedString::from("settings-nav-agents.svg"),
                    SharedString::from("settings-nav-hooks.svg"),
                    SharedString::from("settings-check-updates.svg"),
                ];
                icons.extend(
                    (0..orb_frames::FRAME_COUNT)
                        .map(|index| SharedString::from(format!("norma-orb-{index:02}.png"))),
                );
                icons.extend(
                    (0..orb_frames::FRAME_COUNT).map(|index| {
                        SharedString::from(format!("norma-orb-active-{index:02}.png"))
                    }),
                );
                Ok(icons)
            }
            _ => Ok(vec![]),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/assets_tests.rs"]
mod assets_tests;

#[cfg(test)]
#[path = "../tests/unit/embedded_assets_tests.rs"]
mod tests;

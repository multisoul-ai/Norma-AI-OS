//! Same-window GateRoot: Activation shell or Workbench (`MultisoulApp`).

use super::activate_http::activate_license;
use super::boot::BootPhase;
use super::config::{license_base_url, public_key_hex};
use super::machine_id::current_machine_id;
use super::token_store::{default_token_path, write_token};
#[cfg(target_os = "macos")]
use crate::app::video_wallpaper::{
    decide_video_sync, poll_video_wallpaper, replace_video_wallpaper, video_wallpaper_surface,
    VideoSyncAction, VideoWallpaperState, VIDEO_FOREGROUND_POLL_INTERVAL_MS,
};
#[cfg(target_os = "macos")]
use crate::app::wallpaper_resolvers::resolve_builtin_asset;
use crate::app::{MultisoulApp, QuitNormaOs};
use crate::workspace::windowing::{title_for_initial_state, window_options, InitialWindowState};
use gpui::{
    div, prelude::*, px, App, Context, Entity, FocusHandle, Focusable, SharedString, Window,
    WindowHandle,
};
use license::ActivateLicenseRequest;
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::time::Duration;
use ui::component::input::{Input, InputState};
use ui::component::Root;
use ui::Theme;

#[cfg(target_os = "macos")]
const LICENSE_BACKGROUND_VIDEO_ASSET: &str = "wallpapers/license-gate.mp4";

#[derive(Clone, Debug)]
enum ShellStatus {
    Idle,
    Activating,
    Error(SharedString),
}

pub struct GateRoot {
    phase: BootPhase,
    workbench: Option<Entity<MultisoulApp>>,
    /// Set after successful activate; consumed on next render with a real `Window`.
    enter_workbench: bool,
    license_input: Entity<InputState>,
    status: ShellStatus,
    theme: Theme,
    focus_handle: FocusHandle,
    #[cfg(target_os = "macos")]
    license_video_wallpaper: Option<VideoWallpaperState>,
}

impl GateRoot {
    pub fn new(phase: BootPhase, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let license_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("输入 License Key"));
        let mut root = Self {
            phase,
            workbench: None,
            enter_workbench: false,
            license_input,
            status: ShellStatus::Idle,
            theme: Theme::glass_dark(),
            focus_handle: cx.focus_handle(),
            #[cfg(target_os = "macos")]
            license_video_wallpaper: None,
        };
        if phase == BootPhase::Workbench {
            root.mount_workbench(window, cx);
        }
        root
    }

    fn mount_workbench(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            self.license_video_wallpaper = None;
        }
        let workbench = cx.new(|cx| {
            let mut app = MultisoulApp::new(cx);
            app.start_norma_drain_loop(window, cx);
            app.start_dialogue_turn_loop(window, cx);
            app.start_cef_message_pump_loop(window, cx);
            app
        });
        self.workbench = Some(workbench);
        self.phase = BootPhase::Workbench;
        self.enter_workbench = false;
        cx.notify();
    }

    fn start_activation(&mut self, cx: &mut Context<Self>) {
        if matches!(self.status, ShellStatus::Activating) {
            return;
        }
        let key = self.license_input.read(cx).value().trim().to_string();
        if key.is_empty() {
            self.status = ShellStatus::Error("请输入 License Key".into());
            cx.notify();
            return;
        }

        self.status = ShellStatus::Activating;
        cx.notify();

        let base_url = license_base_url();
        let pubkey = public_key_hex();

        let work = cx.background_executor().spawn(async move {
            let machine_id = current_machine_id();
            let token_path =
                default_token_path().map_err(|err| format!("无法定位数据目录: {err}"))?;
            let request = ActivateLicenseRequest {
                license_key: key,
                identifier: machine_id.clone(),
                identifier_type: "device".into(),
                label: "norma-os-desktop".into(),
            };
            let data = activate_license(&base_url, &request).map_err(|err| err.to_string())?;
            write_token(&token_path, &data.token)
                .map_err(|err| format!("无法保存激活凭证: {err}"))?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let status = license::evaluate_local_license(
                Some(data.token.as_str()),
                &pubkey,
                &machine_id,
                now,
            );
            if !matches!(status, license::LocalLicenseStatus::Activated { .. }) {
                return Err(
                    "激活成功但本地校验失败（公钥与签发服务不匹配；请更新 embed/public_key.hex 或 NORMA_LICENSE_PUBKEY_HEX）"
                        .to_string(),
                );
            }
            Ok(())
        });

        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| match result {
                Ok(()) => {
                    this.status = ShellStatus::Idle;
                    this.enter_workbench = true;
                    cx.notify();
                }
                Err(err) => {
                    this.status = ShellStatus::Error(err.into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    fn sync_license_video_background(&mut self, cx: &mut Context<Self>) {
        let path = license_background_video_path();
        match decide_video_sync(self.license_video_wallpaper.as_ref(), true, path.as_deref()) {
            VideoSyncAction::Keep => {}
            VideoSyncAction::Clear => {
                self.license_video_wallpaper = None;
                cx.notify();
            }
            VideoSyncAction::Restart { path } => {
                replace_video_wallpaper(&mut self.license_video_wallpaper, path.clone());
                cx.notify();
                self.spawn_license_video_poll_loop(path, cx);
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn spawn_license_video_poll_loop(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(VIDEO_FOREGROUND_POLL_INTERVAL_MS))
                .await;
            match this.update(cx, |root, cx| root.poll_license_video_background(&path, cx)) {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    fn poll_license_video_background(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        let (keep_polling, changed) = poll_video_wallpaper(&mut self.license_video_wallpaper, path);
        if changed {
            cx.notify();
        }
        keep_polling
    }

    #[cfg(target_os = "macos")]
    fn render_license_video_layer(&self) -> gpui::Stateful<gpui::Div> {
        let host = div()
            .id("license-video-background")
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .size_full()
            .overflow_hidden();
        match video_wallpaper_surface(self.license_video_wallpaper.as_ref()) {
            Some(frame) => host.child(frame),
            None => host,
        }
    }
}

#[cfg(target_os = "macos")]
fn license_background_video_path() -> Option<PathBuf> {
    resolve_builtin_asset(LICENSE_BACKGROUND_VIDEO_ASSET)
}

impl Focusable for GateRoot {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if let Some(workbench) = &self.workbench {
            return workbench.read(cx).focus_handle(cx);
        }
        self.focus_handle.clone()
    }
}

impl Render for GateRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.enter_workbench && self.workbench.is_none() {
            self.mount_workbench(window, cx);
        }

        if let Some(workbench) = &self.workbench {
            return div()
                .id("gate-workbench")
                .debug_selector(|| "gate-workbench".to_string())
                .size_full()
                .child(workbench.clone())
                .into_any_element();
        }

        #[cfg(target_os = "macos")]
        self.sync_license_video_background(cx);

        let status_text = match &self.status {
            ShellStatus::Idle => SharedString::from(
                "粘贴 Key 后点击激活（需联网）。离线凭证过期时也需重新联网续签。",
            ),
            ShellStatus::Activating => SharedString::from("正在激活…"),
            ShellStatus::Error(msg) => msg.clone(),
        };
        let activating = matches!(self.status, ShellStatus::Activating);
        let theme = self.theme;

        #[cfg(target_os = "macos")]
        let video_background = self.render_license_video_layer();
        #[cfg(not(target_os = "macos"))]
        let video_background = div().size_full();

        let content = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .text_color(gpui::rgb(0xe8f0ec))
            .child(
                div()
                    .text_size(px(28.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("激活 Norma OS"),
            )
            .child(
                div()
                    .id("license-key-input")
                    .debug_selector(|| "license-key-input".to_string())
                    .w(px(420.0))
                    .rounded_md()
                    .bg(gpui::rgba(0x16261fe6))
                    .child(Input::new(&self.license_input).cleanable(true)),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(match &self.status {
                        ShellStatus::Error(_) => gpui::rgb(0xff7777),
                        _ => gpui::rgb(0xb8c9c0),
                    })
                    .child(status_text),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .p(px(4.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(gpui::rgba(0xffffff24))
                    .bg(gpui::rgba(0x05090b52))
                    .child(shell_button(
                        "license-activate-button",
                        if activating { "激活中…" } else { "激活" },
                        LicenseActionStyle::Primary,
                        theme,
                        cx.listener(move |this, _e, _w, cx| {
                            if !activating {
                                this.start_activation(cx);
                            }
                        }),
                    ))
                    .child(shell_button(
                        "license-exit-button",
                        "退出",
                        LicenseActionStyle::Ghost,
                        theme,
                        cx.listener(|_this, _e, _w, cx| cx.quit()),
                    )),
            );

        div()
            .id("activation-shell")
            .debug_selector(|| "activation-shell".to_string())
            .key_context("ActivationShell")
            .track_focus(&self.focus_handle)
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(gpui::rgb(0x0e1b16))
            .child(video_background)
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .bg(gpui::rgba(0x07110d8f)),
            )
            .child(content)
            .on_action(cx.listener(|_this, _: &QuitNormaOs, _w, cx| cx.quit()))
            .into_any_element()
    }
}

#[derive(Clone, Copy)]
enum LicenseActionStyle {
    Primary,
    Ghost,
}

fn shell_button(
    id: &'static str,
    label: &'static str,
    style: LicenseActionStyle,
    theme: Theme,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let (width, background, border, text, hover, active) = match style {
        LicenseActionStyle::Primary => (
            228.0,
            0xfffffff0,
            0xffffffff,
            theme.graphite,
            0xffffffff,
            0xffffffe0,
        ),
        LicenseActionStyle::Ghost => (
            72.0,
            0xffffff00,
            0xffffff00,
            theme.text_secondary,
            0xffffff18,
            0xffffff24,
        ),
    };

    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .w(px(width))
        .h(px(40.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .border_1()
        .border_color(gpui::rgba(border))
        .bg(gpui::rgba(background))
        .text_size(px(14.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(gpui::rgba(text))
        .cursor_pointer()
        .hover(move |button| button.bg(gpui::rgba(hover)))
        .active(move |button| button.bg(gpui::rgba(active)))
        .child(label)
        .on_click(on_click)
}

pub fn open_gated_window(cx: &mut App) -> anyhow::Result<WindowHandle<Root>> {
    let phase = super::resolve::resolve_initial_boot_phase();
    open_gated_window_with_phase(phase, cx)
}

fn open_gated_window_with_phase(
    phase: BootPhase,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<Root>> {
    let title = title_for_initial_state(&InitialWindowState::Launchpad);
    let (width, height) = crate::workspace::windowing::initial_window_dimensions(
        crate::workspace::windowing::visual_preview_name(),
    );
    let bounds = gpui::Bounds::centered(None, gpui::size(px(width), px(height)), cx);
    let handle = cx.open_window(window_options(title, bounds), |window, cx| {
        cx.new(|cx| build_gated_root(phase, window, cx))
    })?;
    Ok(handle)
}

fn build_gated_root(phase: BootPhase, window: &mut Window, cx: &mut Context<Root>) -> Root {
    let gate = cx.new(|cx| GateRoot::new(phase, window, cx));
    Root::new(gate, window, cx)
}

#[cfg(feature = "e2e-test-support")]
pub fn build_activation_root_e2e(window: &mut Window, cx: &mut Context<Root>) -> Root {
    ui::init_components(cx);
    build_gated_root(BootPhase::Activation, window, cx)
}

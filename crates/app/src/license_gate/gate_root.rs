//! Same-window GateRoot: Activation shell or Workbench (`MultisoulApp`).

use super::activate_http::activate_license;
use super::boot::BootPhase;
use super::config::{license_base_url, public_key_hex};
use super::machine_id::current_machine_id;
use super::token_store::{default_token_path, write_token};
use crate::app::{MultisoulApp, QuitNormaOs};
use crate::workspace::windowing::{title_for_initial_state, window_options, InitialWindowState};
use gpui::{
    div, prelude::*, px, App, Context, Entity, FocusHandle, Focusable, SharedString, Window,
    WindowHandle,
};
use license::ActivateLicenseRequest;
use ui::Theme;

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
    license_key: String,
    status: ShellStatus,
    theme: Theme,
    focus_handle: FocusHandle,
}

impl GateRoot {
    pub fn new(phase: BootPhase, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut root = Self {
            phase,
            workbench: None,
            enter_workbench: false,
            license_key: String::new(),
            status: ShellStatus::Idle,
            theme: Theme::glass_dark(),
            focus_handle: cx.focus_handle(),
        };
        if phase == BootPhase::Workbench {
            root.mount_workbench(window, cx);
        }
        root
    }

    fn mount_workbench(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn paste_key_from_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                self.license_key = text.trim().to_string();
                self.status = ShellStatus::Idle;
                cx.notify();
            }
        }
    }

    fn start_activation(&mut self, cx: &mut Context<Self>) {
        if matches!(self.status, ShellStatus::Activating) {
            return;
        }
        let key = self.license_key.trim().to_string();
        if key.is_empty() {
            self.status = ShellStatus::Error("请先粘贴 License Key".into());
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
                    "激活成功但本地校验失败（请检查公钥 NORMA_LICENSE_PUBKEY_HEX）".to_string(),
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
                .size_full()
                .child(workbench.clone())
                .into_any_element();
        }

        let key_preview = if self.license_key.is_empty() {
            "尚未粘贴 License Key".to_string()
        } else if self.license_key.len() <= 12 {
            self.license_key.clone()
        } else {
            format!(
                "{}…{}",
                &self.license_key[..8],
                &self.license_key[self.license_key.len() - 4..]
            )
        };

        let status_text = match &self.status {
            ShellStatus::Idle => SharedString::from(
                "粘贴 Key 后点击激活（需联网）。离线凭证过期时也需重新联网续签。",
            ),
            ShellStatus::Activating => SharedString::from("正在激活…"),
            ShellStatus::Error(msg) => msg.clone(),
        };
        let activating = matches!(self.status, ShellStatus::Activating);
        let theme = self.theme;

        div()
            .id("activation-shell")
            .key_context("ActivationShell")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .bg(gpui::rgb(0x0e1b16))
            .text_color(gpui::rgb(0xe8f0ec))
            .child(
                div()
                    .text_size(px(28.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("激活 Norma OS"),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(gpui::rgb(0x9aada4))
                    .child("未激活无法进入工作台。开发可设 NORMA_SKIP_LICENSE=1。"),
            )
            .child(
                div()
                    .w(px(420.0))
                    .p(px(12.0))
                    .rounded_md()
                    .bg(gpui::rgb(0x16261f))
                    .text_size(px(13.0))
                    .child(key_preview),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(match &self.status {
                        ShellStatus::Error(_) => gpui::rgb(0xff6b6b),
                        _ => gpui::rgb(0x9aada4),
                    })
                    .child(status_text),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(12.0))
                    .child(shell_button(
                        "粘贴 Key",
                        theme,
                        cx.listener(|this, _e, _w, cx| this.paste_key_from_clipboard(cx)),
                    ))
                    .child(shell_button(
                        if activating { "激活中…" } else { "激活" },
                        theme,
                        cx.listener(move |this, _e, _w, cx| {
                            if !activating {
                                this.start_activation(cx);
                            }
                        }),
                    ))
                    .child(shell_button(
                        "退出",
                        theme,
                        cx.listener(|_this, _e, _w, cx| cx.quit()),
                    )),
            )
            .on_action(cx.listener(|_this, _: &QuitNormaOs, _w, cx| cx.quit()))
            .into_any_element()
    }
}

fn shell_button(
    label: &'static str,
    _theme: Theme,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(label))
        .px(px(16.0))
        .py(px(10.0))
        .rounded_md()
        .bg(gpui::rgb(0x2a4a3a))
        .hover(|s| s.bg(gpui::rgb(0x35604a)))
        .cursor_pointer()
        .child(label)
        .on_click(on_click)
}

pub fn open_gated_window(cx: &mut App) -> anyhow::Result<WindowHandle<GateRoot>> {
    let phase = super::resolve::resolve_initial_boot_phase();
    let title = title_for_initial_state(&InitialWindowState::Launchpad);
    let bounds = gpui::Bounds::centered(None, gpui::size(px(1280.0), px(800.0)), cx);
    let handle = cx.open_window(window_options(title, bounds), |window, cx| {
        cx.new(|cx| GateRoot::new(phase, window, cx))
    })?;
    Ok(handle)
}

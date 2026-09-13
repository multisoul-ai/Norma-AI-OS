use crate::mobile::controller::{Command, MobileController};
use gpui::{div, prelude::*, px, rgba, App, Context, Div, Entity, Window};
use storage::PushEnvironment;
use ui::{
    component::{
        button::{Button, ButtonVariants},
        input::{Input, InputState},
        Disableable, Selectable, WindowExt,
    },
    Theme,
};

pub(super) fn entry(theme: Theme, mobile: &Entity<MobileController>, busy: bool) -> Div {
    let mobile = mobile.clone();
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(
            div().flex_1().child("Task Notifications").child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_secondary))
                    .child("Set up notifications for your iPhone"),
            ),
        )
        .child(
            Button::new("mobile-push-settings")
                .debug_selector(|| "mobile-push-settings".into())
                .label("Configure")
                .disabled(busy)
                .on_click(move |_, window, cx| open(mobile.clone(), theme, window, cx)),
        )
}

fn open(mobile: Entity<MobileController>, theme: Theme, window: &mut Window, cx: &mut App) {
    mobile.update(cx, |host, cx| host.request(Command::LoadPush, cx));
    let view = cx.new(|cx| {
        cx.observe(&mobile, |_, _, cx| cx.notify()).detach();
        let endpoint = cx.new(|cx| {
            InputState::new(window, cx).placeholder("https://push.example/v1/notifications")
        });
        let key = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("64-character service access token")
        });
        cx.observe(&endpoint, |_, _, cx| cx.notify()).detach();
        cx.observe(&key, |_, _, cx| cx.notify()).detach();
        PushSettings {
            mobile,
            theme,
            endpoint,
            key,
            environment: PushEnvironment::Sandbox,
        }
    });
    let width = (f32::from(window.viewport_size().width) - 48.0).clamp(260.0, 540.0);
    window.open_dialog(cx, move |dialog, window, _| {
        dialog
            .title("Notification Service")
            .width(px(width))
            .max_h(window.viewport_size().height * 0.8)
            .child(view.clone())
    });
}

struct PushSettings {
    mobile: Entity<MobileController>,
    theme: Theme,
    endpoint: Entity<InputState>,
    key: Entity<InputState>,
    environment: PushEnvironment,
}
impl Render for PushSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let host = self.mobile.read(cx);
        let busy = host.busy;
        let summary = host.push.as_ref();
        let state = if busy {
            "Working…".to_string()
        } else {
            match summary.and_then(|s| s.endpoint.as_ref()) {
                Some(endpoint) => format!(
                    "Configured · {}\n{endpoint}",
                    environment_name(summary.expect("configured").environment)
                ),
                None => "Notification service is not configured".into(),
            }
        };
        let status_id = if summary.and_then(|s| s.endpoint.as_ref()).is_some() {
            "mobile-push-configured"
        } else {
            "mobile-push-status"
        };
        let mut body = div().flex().flex_col().gap(px(14.0)).py(px(12.0))
            .text_size(px(13.0)).text_color(rgba(self.theme.text_primary))
            .child(div().id(status_id).debug_selector(move || status_id.into()).child(state))
            .child(div().text_size(px(12.0)).text_color(rgba(self.theme.text_secondary))
                .child("Settings are stored in this Mac’s Keychain and apply while mobile connection is on. Allow notifications on your iPhone and enable alerts for this Mac."));
        if let Some(error) = &host.error {
            body = body.child(
                div()
                    .id("mobile-push-error")
                    .debug_selector(|| "mobile-push-error".into())
                    .text_color(rgba(self.theme.destructive))
                    .child(error.clone()),
            );
        }
        let can_save = !busy
            && !self.endpoint.read(cx).value().trim().is_empty()
            && !self.key.read(cx).value().trim().is_empty();
        body.child("Service URL")
            .child(
                div()
                    .id("mobile-push-url")
                    .debug_selector(|| "mobile-push-url".into())
                    .child(Input::new(&self.endpoint).disabled(busy)),
            )
            .child("Service Access Token")
            .child(
                div()
                    .id("mobile-push-key")
                    .debug_selector(|| "mobile-push-key".into())
                    .child(Input::new(&self.key).disabled(busy)),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(self.theme.text_secondary))
                    .child("Enter the service token each time you save. Keep the Apple .p8 key on the notification server only."),
            )
            .child("App Environment")
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(self.environment_button(
                        "mobile-push-sandbox",
                        PushEnvironment::Sandbox,
                        busy,
                        cx,
                    ))
                    .child(self.environment_button(
                        "mobile-push-production",
                        PushEnvironment::Production,
                        busy,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(
                        Button::new("mobile-push-save")
                            .debug_selector(|| "mobile-push-save".into())
                            .label("Save")
                            .primary()
                            .disabled(!can_save)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let command = Command::SavePush {
                                    endpoint: this.endpoint.read(cx).value().to_string(),
                                    key: this.key.read(cx).value().to_string(),
                                    environment: this.environment,
                                };
                                this.mobile.update(cx, |host, cx| host.request(command, cx));
                                this.key
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                            })),
                    )
                    .child(
                        Button::new("mobile-push-clear")
                            .debug_selector(|| "mobile-push-clear".into())
                            .label("Clear and Disable")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.mobile
                                    .update(cx, |host, cx| host.request(Command::ClearPush, cx));
                                this.key
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(self.theme.text_secondary))
                    .child("Saving does not send a test notification. Notifications already sent to Apple cannot be recalled."),
            )
    }
}
impl PushSettings {
    fn environment_button(
        &self,
        id: &'static str,
        environment: PushEnvironment,
        busy: bool,
        cx: &Context<Self>,
    ) -> Button {
        Button::new(id)
            .label(environment_name(environment))
            .selected(self.environment == environment)
            .disabled(busy)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.environment = environment;
                cx.notify();
            }))
    }
}
fn environment_name(environment: PushEnvironment) -> &'static str {
    match environment {
        PushEnvironment::Sandbox => "Development · Sandbox",
        PushEnvironment::Production => "TestFlight / App Store",
    }
}

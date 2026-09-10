use crate::{
    app::MultisoulApp,
    mobile::controller::{Command, MobileController},
};
use gpui::{div, img, prelude::*, px, rgba, App, Div, Entity, SharedString};
use storage::MobilePermission;
use ui::{
    component::{button::Button, Disableable, Selectable},
    Theme,
};

mod push;

pub(crate) fn open_dialog(
    app: &mut MultisoulApp,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<MultisoulApp>,
) {
    use ui::component::WindowExt;
    let mobile = app.mobile.clone();
    mobile.update(cx, |host, cx| host.request(Command::Reload, cx));
    let theme = app.theme;
    let view = cx.new(|cx| {
        cx.observe(&mobile, |_, _, cx| cx.notify()).detach();
        MobileSettingsView { mobile, theme }
    });
    let width = (f32::from(window.viewport_size().width) - 48.0).clamp(260.0, 600.0);
    window.open_dialog(cx, move |dialog, window, _| {
        dialog
            .title("手机连接")
            .width(px(width))
            // Dialog already owns a native scroll area; bound it below its 10% top inset.
            .max_h(window.viewport_size().height * 0.8)
            .child(view.clone())
    });
}

struct MobileSettingsView {
    mobile: Entity<MobileController>,
    theme: Theme,
}
impl gpui::Render for MobileSettingsView {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        settings(self.theme, &self.mobile, cx)
    }
}

pub(super) fn settings(theme: Theme, entity: &Entity<MobileController>, cx: &mut App) -> Div {
    let state = entity.read(cx);
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .py(px(24.0))
        .text_size(px(14.0))
        .text_color(rgba(theme.text_primary));
    let status = if state.busy {
        "正在处理…"
    } else if state.enabled() {
        "手机连接已开启"
    } else {
        "手机连接已关闭"
    };
    let mut controls = div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(div().flex_1().child(status));
    if state.enabled() {
        let target = entity.clone();
        controls = controls.child(
            Button::new("mobile-disable")
                .debug_selector(|| "mobile-disable".into())
                .label("关闭连接")
                .on_click(move |_, _, cx| target.update(cx, |host, cx| host.disable(cx))),
        );
    } else {
        controls = controls.child(action(
            entity,
            "mobile-enable",
            "开启手机连接",
            Command::Enable,
            state.busy,
        ));
    }
    body = body.child(controls).child(push::entry(theme, entity, state.busy)).child(div().text_size(px(12.0)).text_color(rgba(theme.text_secondary))
        .child("关闭所有 Norma 窗口会断开手机连接，其中的 Agent 会停止。手机断线不影响电脑继续执行。"));
    if let Some(error) = &state.error {
        body = body.child(
            div()
                .text_color(rgba(theme.destructive))
                .child(error.clone()),
        );
    }
    if state.enabled() {
        body = body.child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_secondary))
                .child(format!("连接地址：{}", state.addresses().join(" · "))),
        );
    }
    if let Some(pending) = &state.pending {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(format!("{} 请求配对", pending.device_name))
                .child(
                    div()
                        .text_size(px(30.0))
                        .child(pending.verification_code.clone()),
                )
                .child(
                    div()
                        .text_color(rgba(theme.text_secondary))
                        .child("请确认 iPhone 显示相同核对码，再选择允许访问的项目。"),
                ),
        );
        body = body.child(projects(theme, entity, cx));
        body = body.child(
            div()
                .flex()
                .gap(px(12.0))
                .child(action(
                    entity,
                    "mobile-approve",
                    "核对一致，批准配对",
                    Command::Approve {
                        request_id: pending.request_id.clone(),
                        projects: state.chosen_projects(),
                    },
                    state.busy,
                ))
                .child(action(
                    entity,
                    "mobile-reject",
                    "拒绝",
                    Command::CancelCode,
                    state.busy,
                )),
        );
    } else if let Some(image) = &state.qr {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(12.0))
                .child("在 iPhone 的 Norma 中选择「添加电脑」，输入验证码或扫码")
                .child("验证码")
                .child(div().id("mobile-code").debug_selector(|| "mobile-code".into())
                    .text_size(px(32.0)).child(state.pairing_code.clone().unwrap_or_default()))
                .child(div().text_size(px(12.0)).text_color(rgba(theme.text_secondary))
                    .child("在手机输入上方电脑地址与 6 位验证码，每组最多验证 3 次。也可以扫描下方二维码。"))
                .child(
                    div()
                        .id("mobile-qr")
                        .debug_selector(|| "mobile-qr".into())
                        .rounded(px(10.0))
                        .bg(gpui::white())
                        .p(px(8.0))
                        .child(img(image.clone()).w(px(144.0)).h(px(144.0))),
                )
                .child(format!(
                    "验证码与二维码将在 {} 秒后失效",
                    state.seconds_left
                ))
                .child(
                    Button::new("mobile-copy-code")
                        .label(if state.code_copied {
                            "已复制验证码"
                        } else {
                            "复制验证码"
                        })
                        .outline()
                        .disabled(state.busy || state.seconds_left == 0)
                        .on_click({
                            let target = entity.clone();
                            move |_, _, cx| {
                                target.update(cx, |host, cx| host.copy_pairing_code(cx))
                            }
                        }),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .child(action(
                            entity,
                            "mobile-new-code",
                            "换一组验证码",
                            Command::NewCode,
                            state.busy,
                        ))
                        .child(action(
                            entity,
                            "mobile-cancel-code",
                            "结束配对",
                            Command::CancelCode,
                            state.busy,
                        )),
                ),
        );
    } else if state.enabled() && state.editing.is_none() {
        body = body.child(action(
            entity,
            "mobile-new-code",
            "配对新手机",
            Command::NewCode,
            state.busy,
        ));
    }
    if let Some((device_id, revision)) = &state.editing {
        body = body
            .child("调整项目权限")
            .child(projects(theme, entity, cx))
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(action(
                        entity,
                        "mobile-save-permissions",
                        "保存权限",
                        Command::Permissions {
                            device_id: device_id.clone(),
                            revision: *revision,
                            projects: state.chosen_projects(),
                        },
                        state.busy,
                    ))
                    .child(action(
                        entity,
                        "mobile-cancel-permissions",
                        "取消",
                        Command::Reload,
                        state.busy,
                    )),
            );
    }
    let mut devices = div().flex().flex_col().gap(px(12.0)).child("已配对的手机");
    for (index, device) in state
        .devices
        .iter()
        .filter(|device| !device.revoked)
        .enumerate()
    {
        let target = entity.clone();
        let editing = device.clone();
        devices = devices.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .py(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(device.device_name.clone())
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(format!(
                                    "{} 个项目 · 权限版本 {}",
                                    device.projects.len(),
                                    device.revision
                                )),
                        ),
                )
                .child(
                    Button::new(("mobile-edit", index))
                        .label("权限")
                        .disabled(state.busy || !state.enabled() || state.qr.is_some())
                        .on_click(move |_, _, cx| {
                            target.update(cx, |host, cx| host.edit(&editing, cx))
                        }),
                )
                .child(action(
                    entity,
                    &format!("mobile-revoke-{index}"),
                    "撤销配对",
                    Command::Revoke(device.device_id.clone()),
                    state.busy,
                )),
        );
    }
    if state.devices.iter().all(|device| device.revoked) {
        devices = devices.child(
            div()
                .text_color(rgba(theme.text_secondary))
                .child("还没有配对的手机"),
        );
    }
    body.child(devices)
}

fn action(
    entity: &Entity<MobileController>,
    id: &str,
    title: &str,
    command: Command,
    disabled: bool,
) -> Button {
    let target = entity.clone();
    let selector = id.to_string();
    Button::new(SharedString::from(id.to_string()))
        .debug_selector(move || selector.clone())
        .label(title.to_string())
        .outline()
        .disabled(disabled)
        .on_click(move |_, _, cx| target.update(cx, |host, cx| host.request(command.clone(), cx)))
}

fn projects(theme: Theme, entity: &Entity<MobileController>, cx: &App) -> Div {
    let host = entity.read(cx);
    let mut list = div().flex().flex_col().gap(px(10.0));
    for project in &host.projects {
        let mut choices = div().flex().gap(px(6.0));
        for (index, (label, access)) in [
            ("不允许", None),
            ("只读", Some(MobilePermission::Read)),
            ("可控制", Some(MobilePermission::Control)),
        ]
        .into_iter()
        .enumerate()
        {
            let target = entity.clone();
            let project_id = project.workspace_id.clone();
            choices = choices.child(
                Button::new(SharedString::from(format!(
                    "mobile-project-{project_id}-{index}"
                )))
                .label(label)
                .selected(host.selected.get(&project_id).copied() == access)
                .disabled(host.busy)
                .on_click(move |_, _, cx| {
                    target.update(cx, |host, cx| {
                        if let Some(access) = access {
                            host.selected.insert(project_id.clone(), access);
                        } else {
                            host.selected.remove(&project_id);
                        }
                        cx.notify();
                    })
                }),
            );
        }
        list = list.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .py(px(8.0))
                .child(project.display_name.clone())
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_secondary))
                        .child(project.canonical_root.display().to_string()),
                )
                .child(choices),
        );
    }
    list.child(
        div()
            .text_size(px(12.0))
            .text_color(rgba(theme.text_secondary))
            .child("未选择项目时，手机只会完成设备配对，不获得任何项目权限。"),
    )
}

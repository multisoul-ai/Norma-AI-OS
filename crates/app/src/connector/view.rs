use super::{Command, Controller, Host};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use gpui::{
    div, prelude::*, px, rgba, App, AppContext, ClipboardItem, Context, Div, Render, SharedString,
    Window,
};
use ui::{
    component::{
        button::{Button, ButtonVariants},
        input::{Input, InputState},
        Disableable, Selectable, WindowExt,
    },
    Theme,
};

impl Render for Controller {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        if self.busy && self.port.is_none() {
            return div().child("连接器正在初始化…");
        }
        let input = self
            .port
            .get_or_insert_with(|| {
                cx.new(|cx| InputState::new(window, cx).default_value(self.config.port.to_string()))
            })
            .clone();
        let tunnel_input = self
            .tunnel
            .get_or_insert_with(|| {
                cx.new(|cx| {
                    InputState::new(window, cx).default_value(
                        self.config
                            .tunnel_authority
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_default(),
                    )
                })
            })
            .clone();
        let status = if self.busy {
            "正在处理…"
        } else if self.listening {
            if self.config.tunnel_authority.is_some() {
                "已开启，允许隧道访问"
            } else {
                "已开启，仅限本机访问"
            }
        } else {
            "已关闭"
        };
        let mut body=div().flex().flex_col().gap(px(18.0)).settings_text(TextRole::Body,theme)
            .child(section(theme,"连接状态").child(div().flex().items_center().gap(px(12.0))
                .child(div().flex_1().child(status))
                .child(action(cx,"connector-toggle",if self.config.enabled {"关闭"} else {"开启"},self.busy,if self.config.enabled {Command::Disable} else {Command::Enable}).selected(self.config.enabled))))
            .child(section(theme,"本机端口").child(div().flex().items_center().gap(px(10.0))
                .child(Input::new(&input).w(px(140.0)).disabled(self.busy))
                .child(Button::new("connector-port-save").child(control_label("保存端口")).outline().disabled(self.busy)
                    .on_click(cx.listener(move |host,_,_,cx| {
                        match input.read(cx).value().parse::<u16>() {
                            Ok(port) if port>0=>host.request(Command::Port(port),cx),
                            _=>{host.error=Some("端口须为 1–65535。".into());cx.notify();}
                        }
                    }))))
                .child(div().settings_text(TextRole::Helper,theme).child("需同步修改隧道配置")))
            .child(section(theme,"隧道地址（可选）").child(div().flex().items_center().gap(px(10.0))
                .child(Input::new(&tunnel_input).flex_1().disabled(self.busy))
                .child(Button::new("connector-tunnel-save").child(control_label("保存地址")).outline().disabled(self.busy)
                    .on_click(cx.listener(move |host,_,_,cx| {
                        let value = tunnel_input.read(cx).value();
                        let value = value.trim();
                        let authority = if value.is_empty() { Ok(None) } else { value.parse::<workspace::HostAuthority>().map(Some) };
                        match authority {
                            Ok(authority)=>host.request(Command::TunnelAuthority(authority),cx),
                            Err(error)=>{host.error=Some(error.into());cx.notify();}
                        }
                    }))))
                .child(div().settings_text(TextRole::Helper,theme).child("填写 Tailscale Funnel 的主机名，可附端口；留空恢复仅本机访问。保存后立即生效。")))
            .child(section(theme,"连接令牌").child(div().flex().gap(px(10.0))
                .child(action(cx,"connector-generate","生成连接令牌",self.busy,Command::Generate))
                .child(action(cx,"connector-revoke","立即吊销",self.busy || self.expiries.is_empty(),Command::Revoke).danger()))
                .child(div().settings_text(TextRole::Helper,theme).child("令牌只在生成时显示一次。轮换后旧令牌保留 24 小时宽限期。关闭连接器会吊销全部令牌。")));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        for expiry in &self.expiries {
            let status = match expiry {
                None => "当前令牌：有效".into(),
                Some(time) => {
                    let minutes = ((time - now).max(0) + 59) / 60;
                    format!(
                        "旧令牌：宽限剩余 {} 小时 {} 分钟",
                        minutes / 60,
                        minutes % 60
                    )
                }
            };
            body = body.child(div().settings_text(TextRole::Helper, theme).child(status));
        }
        if self.locked_until > now {
            body = body.child(div().text_color(rgba(theme.destructive)).child(format!(
                "部分来源 IP 鉴权失败过多，剩余封锁最长 {} 秒；有效令牌仍可使用。",
                self.locked_until - now
            )));
        }
        if let Some(error) = &self.error {
            body = body.child(
                div()
                    .text_color(rgba(theme.destructive))
                    .child(error.clone()),
            );
        }
        let mut audit = section(theme, "最近审计（保留 90 天）");
        for row in &self.audit {
            audit = audit.child(div().settings_text(TextRole::Helper, theme).child(format!(
                "{} 秒前  {}  {}（{} 次）",
                (now - row.occurred_at).max(0),
                row.tool,
                row.result_code,
                row.failure_count
            )));
        }
        body.child(audit)
    }
}
fn section(theme: Theme, title: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(16.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(rgba(theme.node_stroke))
        .child(
            div()
                .settings_text(TextRole::Headline, theme)
                .child(title.to_owned()),
        )
}
fn action(
    cx: &mut Context<Controller>,
    id: &str,
    title: &str,
    busy: bool,
    command: Command,
) -> Button {
    let id = id.to_owned();
    Button::new(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .child(control_label(title.to_owned()))
        .outline()
        .disabled(busy)
        .on_click(cx.listener(move |host, _, _, cx| host.request(command.clone(), cx)))
}
pub(super) fn show_token(token: String, cx: &mut App) {
    let Some(window) = cx.active_window() else {
        return;
    };
    let _ = window.update(cx, |_, window, cx| {
        let theme = cx
            .try_global::<Host>()
            .map(|host| host.0.read(cx).theme)
            .unwrap_or_default();
        window.open_dialog(cx, move |dialog, _, _| {
            let value = token.clone();
            dialog.title("连接令牌（仅显示一次）").child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .settings_text(TextRole::Helper, theme)
                            .child("保存到 Grok Bot 自定义连接器。关闭后无法再次查看。"),
                    )
                    .child(
                        div()
                            .settings_text(TextRole::Body, theme)
                            .child(token.clone()),
                    )
                    .child(
                        Button::new("connector-token-copy")
                            .child(control_label("复制并关闭"))
                            .outline()
                            .on_click(move |_, window, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
                                window.close_dialog(cx);
                            }),
                    ),
            )
        });
    });
}

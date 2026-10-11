//! Exact-card editor. Conflicts stay in this form with the user's draft untouched.
use super::*;
use gpui::{div, prelude::*, px, Context, Entity, Focusable, Render, WeakEntity, Window};
use ui::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    Disableable, WindowExt,
};

struct RoleEditor {
    host: WeakEntity<MultisoulApp>,
    card: WorkspaceCardRef,
    input: Entity<InputState>,
    store: storage::WorkspaceAgentRoleStore,
    database: std::path::PathBuf,
    canvas: storage::CanvasStore,
    busy: bool,
    error: Option<String>,
    enabled: bool,
    repair: Option<String>,
    theme: ui::Theme,
}

impl MultisoulApp {
    pub(crate) fn open_workspace_role_editor(
        &mut self,
        node: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = self.workspace_card_ref(node) else {
            return;
        };
        let Some(project) = self.persistence.project_store() else {
            return;
        };
        let canvas::CanvasNodeKind::Agent(spec) = &self.runtime.document.node(node).unwrap().kind
        else {
            return;
        };
        let name = spec.alias.clone().unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let focus = input.focus_handle(cx);
        let store =
            storage::WorkspaceAgentRoleStore::new(self.database_paths.app_db_path().to_owned());
        let host = cx.weak_entity();
        let canvas = project.canvas_store();
        let view = cx.new(|_| RoleEditor {
            host,
            card,
            input,
            store,
            database: self.database_paths.app_db_path().to_owned(),
            canvas,
            busy: true,
            error: None,
            enabled: true,
            repair: None,
            theme: self.theme,
        });
        view.update(cx, |form, cx| {
            let input = form.input.clone();
            cx.subscribe_in(&input, window, |form, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    form.save(false, window, cx);
                }
            })
            .detach();
            let store = form.store.clone();
            let card = form.card.clone();
            let read = cx.background_executor().spawn(async move {
                let repair = store
                    .repairs(&card.workspace_id)?
                    .into_iter()
                    .find(|r| r.card == card);
                Ok::<_, storage::StorageError>((store.get(&card)?, repair))
            });
            cx.spawn(async move |this, cx| {
                let result = read.await;
                let _ = this.update(cx, |form, cx| {
                    form.busy = false;
                    match result {
                        Ok((Some(role), repair)) => {
                            form.enabled = role.enabled;
                            form.repair = repair.map(|r| {
                                format!(
                                    "自动修复记录：{} → {}",
                                    r.previous_name.unwrap_or_else(|| "未命名".into()),
                                    r.name
                                )
                            });
                        }
                        _ => form.error = Some("角色登记尚未就绪，请重新打开。".into()),
                    };
                    cx.notify();
                });
            })
            .detach();
        });
        window.open_dialog(cx, move |dialog, window, _| {
            dialog
                .title("卡片角色")
                .width(px(420.0).min(window.viewport_size().width - px(40.0)))
                .overlay_closable(false)
                .close_button(false)
                .keyboard(false)
                .child(view.clone())
        });
        focus.focus(window);
    }
}

impl RoleEditor {
    fn save(&mut self, toggle: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let name = self.input.read(cx).value().to_string();
        if !toggle && agent::AgentName::parse(&name).is_err() {
            self.error = Some("名称须为 2–16 个英文字母或 2–8 个汉字；Norma 为保留名称。".into());
            cx.notify();
            return;
        }
        self.busy = true;
        self.error = None;
        let publication = self
            .host
            .upgrade()
            .map(|host| host.update(cx, |app, _| app.persistence.begin_card_publication()));
        cx.notify();
        let card = self.card.clone();
        let store = self.store.clone();
        let database = self.database.clone();
        let canvas = self.canvas.clone();
        let enabled = !self.enabled;
        let host = self.host.clone();
        let work = cx.background_executor().spawn(async move {
            let role = if toggle {
                super::super::dispatch::authority::set_role_enabled(&database, &card, enabled)?
            } else {
                store.rename(&card, &name)?
            };
            let warning = canvas
                .patch_agent_aliases(&[(card.node_id, card.card_instance_id, role.name.clone())])
                .err()
                .map(|e| format!("名称已保存，持久标题待重试：{e}"));
            Ok::<_, storage::StorageError>((role, warning))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            if let Some(host) = host.upgrade() {
                let role = result.as_ref().ok().map(|(role, _)| role.clone());
                let _ = host.update(cx, |app, cx| {
                    if let Some(role) = role {
                        if project_role_title(&mut app.runtime, &role) {
                            app.mark_canvas_dirty();
                        }
                    }
                    if let Some(publication) = publication.as_ref() {
                        app.persistence.end_card_publication(publication);
                    }
                    cx.notify();
                });
            }
            let _ = this.update_in(cx, |form, window, cx| {
                form.busy = false;
                match result {
                    Ok((role, warning)) => {
                        form.enabled = role.enabled;
                        let synchronized = warning.is_none();
                        form.error = warning;
                        if !toggle && synchronized {
                            window.close_dialog(cx);
                        }
                    }
                    Err(storage::StorageError::AgentNameConflict {existing,..}) => {
                        form.error=Some(format!("此称呼已被「{existing}」占用（含失效旧称呼）。请保留任务并选择其他名称。"));
                    }
                    Err(error) => {form.error = Some(error.to_string());}
                }
                cx.notify();
            });
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }
}

impl Render for RoleEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child("这个名称在整个工作空间内唯一。改名后旧称呼失效并保留占用。")
            .child(Input::new(&self.input).disabled(self.busy))
            .when_some(self.repair.clone(), |body, repair| {
                body.child(div().text_size(px(12.0)).child(repair))
            })
            .when_some(self.error.clone(), |body, error| {
                body.child(
                    div()
                        .text_color(gpui::rgba(self.theme.destructive))
                        .child(error),
                )
            })
            .child(
                Button::new("role-cancel")
                    .label("取消")
                    .disabled(self.busy)
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        Button::new("role-enabled")
                            .label(if self.enabled {
                                "停用角色"
                            } else {
                                "启用角色"
                            })
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.save(true, window, cx)),
                            ),
                    )
                    .child(
                        Button::new("role-save")
                            .primary()
                            .label(if self.busy {
                                "保存中…"
                            } else {
                                "保存名称"
                            })
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.save(false, window, cx)),
                            ),
                    ),
            )
    }
}

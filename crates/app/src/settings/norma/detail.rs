use super::document::body_text;
use super::*;
use gpui::{div, prelude::*, px, rgba, Focusable};
use ui::component::StyledExt;
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable, Sizable,
};
use ui::icons::with_alpha;

fn source_time(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(timestamp, |time| time.as_secs());
    let age = now.saturating_sub(timestamp);
    if age < 60 {
        "刚刚".into()
    } else if age < 3600 {
        format!("{} 分钟前", age / 60)
    } else if age < 86400 {
        format!("{} 小时前", age / 3600)
    } else {
        format!("{} 天前", age / 86400)
    }
}

impl MemoryView {
    pub(super) fn apply_entry(
        &mut self,
        entry: agent::MemoryEntry,
        sources: Vec<storage::MemorySourceDetail>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected = Some(entry.summary.id.as_str().into());
        self.original = body_text(&entry.markdown);
        self.original_title = entry.summary.name;
        self.original_description = entry.summary.description;
        self.kind = entry.summary.kind;
        self.modified = entry.summary.modified;
        self.sources = sources;
        self.editing = false;
        self.latest = None;
        self.id_input.update(cx, |input, cx| {
            input.set_value(entry.summary.id.as_str().to_string(), window, cx)
        });
        self.title_input.update(cx, |input, cx| {
            input.set_value(self.original_title.clone(), window, cx)
        });
        self.description_input.update(cx, |input, cx| {
            input.set_value(self.original_description.clone(), window, cx)
        });
        self.body.update(cx, |input, cx| {
            input.set_value(self.original.clone(), window, cx)
        });
    }

    pub(super) fn detail(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme;
        let mut detail = div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .rounded_tr(px(10.0))
            .rounded_br(px(10.0))
            .bg(rgba(theme.panel_high));
        if !self.imports.is_empty() {
            return detail.child(self.import_preview(cx));
        }
        if self.selected.is_none() && !self.editing {
            return detail
                .items_center()
                .justify_center()
                .gap_3()
                .p_6()
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_medium()
                        .child("让 Norma 更了解你"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_secondary))
                        .text_center()
                        .child("选择一条记忆查看内容与来源，\n或写下希望她记住的偏好。"),
                )
                .child(
                    Button::new("memory-empty-new")
                        .small()
                        .label("新增记忆")
                        .disabled(self.busy)
                        .on_click(cx.listener(|view, _, window, cx| view.new_entry(window, cx))),
                );
        }
        let title = self.title_input.read(cx).value().to_string();
        detail = detail.child(
            div()
                .flex_none()
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(rgba(with_alpha(theme.text_secondary, 0x20)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(div().text_size(px(15.0)).font_semibold().child(
                            if self.selected.is_none() {
                                "新增记忆".into()
                            } else {
                                title.clone()
                            },
                        ))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(if self.editing {
                                    "未保存".into()
                                } else {
                                    format!("已保存 · 版本 {}", self.version.number())
                                }),
                        ),
                )
                .child(
                    div()
                        .mt_1()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_secondary))
                        .child(self.scope.label()),
                ),
        );
        let mut body = div()
            .id("memory-detail-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_3();
        if self.editing {
            body = body
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_secondary))
                        .child("名称"),
                )
                .child(super::view::editor(
                    "memory-title-input",
                    &self.title_input,
                    self.busy || self.pending_save.is_some(),
                    None,
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_secondary))
                        .child("何时使用（可选）"),
                )
                .child(super::view::editor(
                    "memory-description-input",
                    &self.description_input,
                    self.busy || self.pending_save.is_some(),
                    None,
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_secondary))
                        .child("记忆内容 · 支持 Markdown"),
                )
                .child(super::view::editor(
                    "memory-body-input",
                    &self.body,
                    self.busy || self.pending_save.is_some(),
                    Some(190.0),
                    cx,
                ));
        } else {
            let description = self.description_input.read(cx).value().to_string();
            body = body
                .when(!description.is_empty(), |body| {
                    body.child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgba(theme.text_secondary))
                            .child(description),
                    )
                })
                .child(
                    div()
                        .text_size(px(14.0))
                        .whitespace_normal()
                        .line_height(px(24.0))
                        .child(self.body.read(cx).value().to_string()),
                );
            for source in &self.sources {
                body = body.child(
                    div()
                        .mt_4()
                        .pt_3()
                        .border_t_1()
                        .border_color(rgba(with_alpha(theme.text_secondary, 0x20)))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(format!("来源 · {}", source_time(source.recorded_at))),
                        )
                        .child(
                            div()
                                .mt_2()
                                .text_size(px(12.0))
                                .whitespace_normal()
                                .child(source.text.clone()),
                        ),
                );
            }
        }
        if let Some((version, latest)) = &self.latest {
            let version = *version;
            body = body.child(
                div()
                    .p_3()
                    .rounded(px(8.0))
                    .bg(rgba(with_alpha(theme.accent_orange, 0x14)))
                    .child(div().font_medium().child("其他操作更新了这条记忆"))
                    .child(
                        div()
                            .mt_2()
                            .text_size(px(12.0))
                            .child(format!("最新名称：{}", latest.summary.name)),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(12.0))
                            .child(format!("最新用途：{}", latest.summary.description)),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_size(px(12.0))
                            .child(body_text(&latest.markdown)),
                    )
                    .child(
                        Button::new("memory-rebase")
                            .small()
                            .label("已比较，使用最新版本保存草稿")
                            .disabled(self.busy || self.pending_save.is_some())
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.version = version;
                                view.latest = None;
                                view.status = "保存基线已更新，草稿仍保留。".into();
                                cx.notify();
                            })),
                    ),
            );
        }
        detail.child(body).child(
            div()
                .flex_none()
                .p_3()
                .border_t_1()
                .border_color(rgba(with_alpha(theme.text_secondary, 0x20)))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_secondary))
                        .child(if self.editing {
                            "保存后，Norma 可在对话中按需读取。"
                        } else {
                            "纠正会发布新版本，并保留历史。"
                        }),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .when(self.editing, |row| {
                            row.child(
                                Button::new("memory-discard")
                                    .small()
                                    .ghost()
                                    .label("取消")
                                    .disabled(self.busy || self.pending_save.is_some())
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.discard(window, cx)),
                                    ),
                            )
                            .child(
                                Button::new("memory-commit")
                                    .small()
                                    .primary()
                                    .label(if !self.busy && self.pending_save.is_some() {
                                        "核验保存结果"
                                    } else if self.saving {
                                        "保存中…"
                                    } else {
                                        "保存"
                                    })
                                    .disabled(
                                        self.busy
                                            || title.trim().is_empty()
                                            || self.body.read(cx).value().trim().is_empty(),
                                    )
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.save(window, cx)),
                                    ),
                            )
                        })
                        .when(!self.editing, |row| {
                            row.child(
                                Button::new("memory-edit")
                                    .small()
                                    .label("编辑 / 纠正")
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.editing = true;
                                        view.body.focus_handle(cx).focus(window);
                                        cx.notify();
                                    })),
                            )
                        }),
                ),
        )
    }
    fn import_preview(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let mut preview = div()
            .id("memory-import-preview")
            .size_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .p_4()
                    .font_semibold()
                    .child(format!("导入预览 · {} 条", self.imports.len())),
            );
        let mut documents = div()
            .id("memory-import-documents")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_4();
        for (id, markdown) in &self.imports {
            documents = documents.child(
                div()
                    .child(div().font_medium().child(format!(
                        "{id} · {}",
                        if self.entries.iter().any(|entry| entry.id.as_str() == id) {
                            "覆盖现有主题"
                        } else {
                            "新增主题"
                        }
                    )))
                    .child(
                        div()
                            .mt_2()
                            .text_size(px(12.0))
                            .whitespace_normal()
                            .child(markdown.clone()),
                    ),
            );
        }
        preview = preview.child(documents).child(
            div()
                .p_3()
                .flex_none()
                .flex()
                .gap_2()
                .justify_end()
                .child(
                    Button::new("memory-import-cancel")
                        .small()
                        .ghost()
                        .label("取消")
                        .disabled(self.busy || self.pending_save.is_some())
                        .on_click(cx.listener(|view, _, window, cx| view.discard(window, cx))),
                )
                .child(
                    Button::new("memory-import-commit")
                        .small()
                        .primary()
                        .label(if self.pending_save.is_some() {
                            "核验保存结果"
                        } else {
                            "确认导入全部"
                        })
                        .disabled(self.busy)
                        .on_click(cx.listener(|view, _, window, cx| view.save(window, cx))),
                ),
        );
        preview
    }
}

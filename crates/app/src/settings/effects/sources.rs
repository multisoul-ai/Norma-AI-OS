//! 按派发来源选特效：三行，每行一个 Material 风格菜单（48px 选项、勾选 = 已保存值、高亮 = 键盘浏览）。
//! 键盘：Enter/Space/↓ 打开，↑↓ 浏览，Enter 确认，Esc 或点击外部关闭且不改变设置。

use super::page::EffectsPageData;
use super::state::{Source, GRID_STYLES};
use crate::app::effects_actions::{style_line, style_name};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use gpui::{
    div, prelude::*, px, rgba, App, Context, Corner, Div, KeyDownEvent, RenderOnce, Window,
};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    popover::Popover,
    Icon, IconName, Selectable,
};
use ui::Theme;

const SOURCES: [Source; 3] = [Source::Voice, Source::Idea, Source::Handoff];

/// 菜单选项：`None` = 跟随当前特效。
pub(crate) fn options(source: Source) -> Vec<Option<&'static str>> {
    let mut list = vec![None];
    if source == Source::Handoff {
        list.push(Some("ion"));
    }
    list.extend(GRID_STYLES.iter().map(|id| Some(*id)));
    list
}

fn saved(data: &EffectsPageData, source: Source) -> Option<String> {
    let pref = data
        .target
        .as_ref()
        .and_then(|target| data.state.effective_for(target));
    pref.and_then(|pref| match source {
        Source::Voice => pref.voice.clone(),
        Source::Idea => pref.idea.clone(),
        Source::Handoff => pref.handoff.clone(),
    })
}

fn title_key(source: Source) -> (&'static str, &'static str) {
    match source {
        Source::Voice => (
            ui::i18n::text("effects.source_voice"),
            ui::i18n::text("effects.source_voice_sub"),
        ),
        Source::Idea => (
            ui::i18n::text("effects.source_idea"),
            ui::i18n::text("effects.source_idea_sub"),
        ),
        Source::Handoff => (
            ui::i18n::text("effects.source_handoff"),
            ui::i18n::text("effects.source_handoff_sub"),
        ),
    }
}

pub(super) fn sources_section(
    theme: Theme,
    data: &EffectsPageData,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let enabled = data.target.is_some()
        && data
            .target
            .as_ref()
            .is_some_and(|t| !data.state.follows_workspace(t));
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .settings_font(TextRole::Headline)
                .child(ui::i18n::text("effects.sources")),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("effects.sources_hint")),
        )
        .child(
            div()
                .rounded(px(16.0))
                .border_1()
                .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x60))
                .bg(rgba(theme.panel_high))
                .children(SOURCES.iter().enumerate().map(|(i, source)| {
                    source_row(theme, data, *source, i > 0, enabled, window, cx)
                })),
        )
}

fn source_row(
    theme: Theme,
    data: &EffectsPageData,
    source: Source,
    divider: bool,
    enabled: bool,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let (title, subtitle) = title_key(source);
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(16.0))
        .min_h(px(64.0))
        .px(px(16.0))
        .py(px(10.0))
        .when(divider, |row| {
            row.border_t_1()
                .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x40))
        })
        .child(
            div()
                .flex_1()
                .min_w(px(180.0))
                .flex()
                .flex_col()
                .child(div().settings_font(TextRole::RowTitle).child(title))
                .child(div().settings_text(TextRole::Helper, theme).child(subtitle)),
        )
        .child(picker(theme, data, source, enabled, window, cx))
}

fn option_label(value: Option<&str>) -> String {
    match value {
        None => ui::i18n::text("effects.follow_current").to_owned(),
        Some(id) => style_name(id).to_owned(),
    }
}

fn picker(
    theme: Theme,
    data: &EffectsPageData,
    source: Source,
    enabled: bool,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> gpui::Stateful<Div> {
    let open = enabled && data.menu == Some(source);
    let current = saved(data, source);
    let list = options(source);
    let cursor = data.menu_cursor.min(list.len() - 1);
    let followed = data
        .target
        .as_ref()
        .map(|t| data.state.selected_style(t).to_owned())
        .unwrap_or_default();
    let app = cx.weak_entity();
    let key_app = app.clone();
    let open_app = app.clone();
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba((theme.accent & 0xffffff00) | if open { 0x20 } else { 0x0d }).into())
        .foreground(rgba(theme.text_primary).into())
        .border(rgba((theme.text_primary & 0xffffff00) | 0x20).into())
        .hover(rgba((theme.accent & 0xffffff00) | 0x24).into())
        .active(rgba((theme.accent & 0xffffff00) | 0x38).into());
    let surface = (theme.panel_high & 0xffffff00) | 0xff;
    let menu_width = px(300.0).min(window.viewport_size().width - px(40.0));
    let label = option_label(current.as_deref());
    let rows: Vec<MenuRow> = list
        .iter()
        .enumerate()
        .map(|(index, value)| MenuRow {
            theme,
            source,
            value: *value,
            label: option_label(*value),
            detail: value.map_or_else(
                || {
                    ui::i18n::format(
                        "effects.follow_current_now",
                        &[("style", style_name(&followed).to_owned())],
                    )
                },
                |id| style_line(id).to_owned(),
            ),
            checked: *value == current.as_deref(),
            highlighted: index == cursor,
            app: app.clone(),
        })
        .collect();
    div()
        .id(("effects-source-picker", source as usize))
        .debug_selector(move || format!("effects-source-{source:?}").to_lowercase())
        .w(px(240.0))
        .max_w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(0x00000000))
        .focus(move |style| style.border_color(rgba(theme.accent)))
        .when(!enabled, |picker| picker.opacity(0.5))
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if !enabled {
                return;
            }
            let key = event.keystroke.key.as_str();
            let handled = key_app
                .update(cx, |app, cx| app.handle_effect_menu_key(source, key, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        })
        .child(
            Popover::new(("effects-source-popover", source as usize))
                .anchor(Corner::BottomRight)
                .open(open)
                .appearance(false)
                .w(menu_width)
                .py(px(6.0))
                .bg(rgba(surface))
                .rounded(px(12.0))
                .shadow_md()
                .border_1()
                .border_color(rgba((theme.text_primary & 0xffffff00) | 0x18))
                .on_open_change(move |is_open, _, cx| {
                    let _ =
                        open_app.update(cx, |app, cx| app.set_effect_menu(source, *is_open, cx));
                })
                .trigger(Trigger {
                    button: Button::new(("effects-source-trigger", source as usize))
                        .custom(variant)
                        .tab_stop(false)
                        .w_full()
                        .h(px(40.0))
                        .rounded(px(8.0))
                        .justify_start()
                        .px(px(12.0))
                        .child(
                            div()
                                .max_w(px(176.0))
                                .settings_font(TextRole::Body)
                                .truncate()
                                .child(label),
                        ),
                    icon: Icon::new(if open {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    })
                    .size(px(16.0))
                    .text_color(rgba(theme.text_secondary)),
                })
                .content(move |_, _, _| div().flex().flex_col().children(rows.clone())),
        )
}

// 按钮内部的标签自带宽度，箭头单独贴在右侧，同时保留按钮自身的交互状态。
#[derive(IntoElement)]
struct Trigger {
    button: Button,
    icon: Icon,
}

impl Selectable for Trigger {
    fn selected(mut self, selected: bool) -> Self {
        self.button = self.button.selected(selected);
        self
    }

    fn is_selected(&self) -> bool {
        self.button.is_selected()
    }
}

impl RenderOnce for Trigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().relative().w_full().child(self.button).child(
            div()
                .absolute()
                .right(px(12.0))
                .top(px(12.0))
                .child(self.icon),
        )
    }
}

#[derive(Clone, IntoElement)]
struct MenuRow {
    theme: Theme,
    source: Source,
    value: Option<&'static str>,
    label: String,
    detail: String,
    checked: bool,
    highlighted: bool,
    app: gpui::WeakEntity<MultisoulApp>,
}

impl RenderOnce for MenuRow {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let (source, value, app) = (self.source, self.value, self.app);
        let accent = |alpha: u32| rgba((theme.accent & 0xffffff00) | alpha);
        div()
            .id(gpui::SharedString::from(format!(
                "effects-menu-{source:?}-{}",
                value.unwrap_or("follow")
            )))
            .debug_selector(move || format!("effects-menu-{}", value.unwrap_or("follow")))
            .w_full()
            .h(px(48.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .border_1()
            .border_color(accent(if self.highlighted { 0x70 } else { 0 }))
            .bg(accent(if self.highlighted {
                0x20
            } else if self.checked {
                0x14
            } else {
                0
            }))
            .cursor_pointer()
            .hover(move |style| style.bg(accent(0x28)))
            .active(move |style| style.bg(accent(0x38)))
            .on_click(move |_, _, cx| {
                let _ = app.update(cx, |app, cx| app.choose_effect_source(source, value, cx));
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .settings_text(TextRole::RowTitle, theme)
                            .truncate()
                            .child(self.label),
                    )
                    .child(
                        div()
                            .settings_text(TextRole::Caption, theme)
                            .truncate()
                            .child(self.detail),
                    ),
            )
            .child(div().flex_none().w(px(20.0)).when(self.checked, |tail| {
                tail.child(
                    Icon::new(IconName::Check)
                        .size(px(16.0))
                        .text_color(rgba(theme.accent)),
                )
            }))
    }
}

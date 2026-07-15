use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, Stateful, Styled};
use ui::{
    component::{Icon, Sizable as _},
    Theme,
};

use super::{with_alpha, TOP_BAR_NAV_HEIGHT};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum SettingsNavigationMode {
    Sidebar,
    CompactSidebar,
    TopBar,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum SettingsSection {
    #[cfg(test)]
    General,
    Keyboard,
    #[cfg(test)]
    Runtime,
    Voice,
    Agents,
    #[cfg(test)]
    Skills,
    Hooks,
    Wallpaper,
}

#[derive(Clone, Copy)]
struct SettingsSectionMetadata {
    title: &'static str,
    icon_asset: Option<&'static str>,
    index: usize,
    debug_selector: &'static str,
}

impl SettingsSection {
    pub(crate) fn title(self) -> &'static str {
        self.metadata().title
    }

    fn metadata(self) -> SettingsSectionMetadata {
        match self {
            #[cfg(test)]
            Self::General => SettingsSectionMetadata {
                title: "General",
                icon_asset: None,
                index: 0,
                debug_selector: "settings-section-general",
            },
            Self::Keyboard => SettingsSectionMetadata {
                title: "Keyboard Shortcuts",
                icon_asset: Some("icons/settings-nav-keyboard.svg"),
                index: 1,
                debug_selector: "settings-section-keyboard",
            },
            #[cfg(test)]
            Self::Runtime => SettingsSectionMetadata {
                title: "Runtime",
                icon_asset: None,
                index: 2,
                debug_selector: "settings-section-runtime",
            },
            Self::Voice => SettingsSectionMetadata {
                title: "Voice",
                icon_asset: Some("icons/settings-nav-voice.svg"),
                index: 3,
                debug_selector: "settings-section-voice",
            },
            Self::Agents => SettingsSectionMetadata {
                title: "Agents",
                icon_asset: Some("icons/settings-nav-agents.svg"),
                index: 4,
                debug_selector: "settings-section-agents",
            },
            #[cfg(test)]
            Self::Skills => SettingsSectionMetadata {
                title: "Skills",
                icon_asset: None,
                index: 5,
                debug_selector: "settings-section-skills",
            },
            Self::Hooks => SettingsSectionMetadata {
                title: "Agent Hooks",
                icon_asset: Some("icons/settings-nav-hooks.svg"),
                index: 6,
                debug_selector: "settings-section-hooks",
            },
            Self::Wallpaper => SettingsSectionMetadata {
                title: "Wallpaper",
                icon_asset: Some("icons/settings-nav-wallpaper.svg"),
                index: 7,
                debug_selector: "settings-section-wallpaper",
            },
        }
    }
}

const APPEARANCE_SECTIONS: &[SettingsSection] = &[SettingsSection::Wallpaper];
const INPUT_SECTIONS: &[SettingsSection] = &[SettingsSection::Keyboard, SettingsSection::Voice];
const AUTOMATION_SECTIONS: &[SettingsSection] = &[SettingsSection::Agents, SettingsSection::Hooks];

const NAVIGATION_GROUPS: [(&str, &[SettingsSection]); 3] = [
    ("Appearance", APPEARANCE_SECTIONS),
    ("Input & Controls", INPUT_SECTIONS),
    ("AI & Automation", AUTOMATION_SECTIONS),
];

pub(super) fn settings_navigation(
    theme: Theme,
    active: SettingsSection,
    mode: SettingsNavigationMode,
    sidebar_width: f32,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    match mode {
        SettingsNavigationMode::Sidebar | SettingsNavigationMode::CompactSidebar => {
            sidebar_navigation(theme, active, mode, sidebar_width, cx)
        }
        SettingsNavigationMode::TopBar => top_bar_navigation(theme, active, cx),
    }
}

fn sidebar_navigation(
    theme: Theme,
    active: SettingsSection,
    mode: SettingsNavigationMode,
    sidebar_width: f32,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let compact = mode == SettingsNavigationMode::CompactSidebar;
    let mut navigation = div()
        .id("settings-sidebar-scroll")
        .debug_selector(|| "settings-sidebar-scroll".to_string())
        .w(px(sidebar_width))
        .min_w(px(sidebar_width))
        .h_full()
        .overflow_y_scroll()
        .overflow_x_hidden()
        .flex()
        .flex_col()
        .gap(px(if compact { 14.0 } else { 18.0 }))
        .px(px(if compact { 12.0 } else { 18.0 }))
        .py(px(if compact { 16.0 } else { 22.0 }))
        .border_r_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .bg(rgba(with_alpha(theme.node_fill, 0xf2)))
        .child(
            div()
                .mb(px(if compact { 2.0 } else { 6.0 }))
                .text_size(px(if compact { 20.0 } else { 24.0 }))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child("Settings"),
        );

    for (group, sections) in NAVIGATION_GROUPS {
        navigation = navigation.child(sidebar_group(theme, group, sections, active, compact, cx));
    }
    navigation
}

fn sidebar_group(
    theme: Theme,
    title: &'static str,
    sections: &[SettingsSection],
    active: SettingsSection,
    compact: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(
            div()
                .px(px(8.0))
                .text_size(px(if compact { 10.0 } else { 11.0 }))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_tertiary))
                .child(title),
        )
        .children(
            sections
                .iter()
                .copied()
                .map(|section| navigation_item(theme, section, active, false, cx)),
        )
}

fn top_bar_navigation(
    theme: Theme,
    active: SettingsSection,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let mut navigation = div()
        .id("settings-navigation-topbar")
        .debug_selector(|| "settings-navigation-topbar".to_string())
        .w_full()
        .min_h(px(TOP_BAR_NAV_HEIGHT))
        .flex_none()
        .overflow_x_scroll()
        .overflow_y_hidden()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(14.0))
        .px(px(14.0))
        .py(px(12.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .bg(rgba(with_alpha(theme.node_fill, 0xf2)));

    for (group, sections) in NAVIGATION_GROUPS {
        navigation = navigation.child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(
                    div()
                        .px(px(6.0))
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_tertiary))
                        .child(group),
                )
                .child(
                    div().flex().flex_row().gap(px(5.0)).children(
                        sections
                            .iter()
                            .copied()
                            .map(|section| navigation_item(theme, section, active, true, cx)),
                    ),
                ),
        );
    }
    navigation
}

fn navigation_item(
    theme: Theme,
    section: SettingsSection,
    active: SettingsSection,
    compact: bool,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let metadata = section.metadata();
    let is_active = section == active;
    let background = if is_active {
        with_alpha(theme.accent_orange, 0x1c)
    } else {
        0x00000000
    };
    let text = if is_active {
        theme.text_primary
    } else {
        theme.text_secondary
    };

    div()
        // gpui-component 0.5.1's Button does not expose GPUI's `debug_selector` seam.
        // Keep this custom interactive wrapper so existing L2 click tests target the same
        // public UI contract; the icon itself is the component library's Icon element.
        .id(("settings-section", metadata.index))
        .debug_selector(move || metadata.debug_selector.to_string())
        .h(px(40.0))
        .when(compact, |item| item.flex_none().min_w(px(132.0)))
        .when(!compact, |item| item.w_full())
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.0))
        .px(px(10.0))
        .rounded(px(9.0))
        .bg(rgba(background))
        .text_color(rgba(text))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xa6))))
        .on_click(cx.listener(move |app, _, _window, cx| {
            app.settings_panel.select(section);
            if section == SettingsSection::Voice {
                app.refresh_asr_model_states(cx);
            }
            cx.notify();
        }))
        .child(
            div()
                .w(px(2.0))
                .h(px(18.0))
                .rounded(px(2.0))
                .bg(rgba(if is_active {
                    theme.accent_orange
                } else {
                    0x00000000
                })),
        )
        .child(section_icon(theme, section, is_active))
        .child(
            div()
                .min_w(px(0.0))
                .overflow_hidden()
                .truncate()
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM)
                .child(section.title()),
        )
}

fn section_icon(theme: Theme, section: SettingsSection, active: bool) -> Icon {
    let icon_asset = section
        .metadata()
        .icon_asset
        .expect("only visible Settings sections render navigation icons");
    Icon::empty()
        .path(icon_asset)
        .with_size(px(16.0))
        .text_color(rgba(if active {
            theme.accent_orange
        } else {
            theme.text_secondary
        }))
}

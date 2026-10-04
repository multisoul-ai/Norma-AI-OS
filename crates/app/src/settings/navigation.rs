use crate::app::MultisoulApp;
use crate::assets::{
    APP_ICON_ASSET, SETTINGS_CHECK_UPDATES_ASSET, SETTINGS_NAV_THEMES_ASSET, TOOLBAR_SETTINGS_ASSET,
};
use gpui::{div, img, prelude::*, px, rgba, Context, Div, FontWeight, ObjectFit, Stateful, Styled};
use ui::{
    component::{
        button::{Button, ButtonVariants as _},
        Icon, Sizable as _, Size,
    },
    Theme,
};

use super::{with_alpha, TOP_BAR_NAV_HEIGHT};
#[path = "navigation/brand.rs"]
mod brand;
use brand::settings_brand_footer;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum SettingsNavigationMode {
    Sidebar,
    CompactSidebar,
    TopBar,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct SettingsBrandMetadata {
    product_name: &'static str,
    version: &'static str,
    icon_asset: &'static str,
}

fn settings_brand_metadata() -> SettingsBrandMetadata {
    SettingsBrandMetadata {
        product_name: "Norma OS",
        version: env!("CARGO_PKG_VERSION"),
        icon_asset: APP_ICON_ASSET,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum SettingsSection {
    Themes,
    Keyboard,
    #[cfg(test)]
    Runtime,
    NormaMemory,
    NormaVoice,
    NormaPersona,
    Agents,
    #[cfg(test)]
    Skills,
    Orchestration,
    Wallpaper,
    Performance,
    Mobile,
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

    pub(crate) fn is_norma(self) -> bool {
        matches!(
            self,
            Self::NormaMemory | Self::NormaVoice | Self::NormaPersona
        )
    }

    fn metadata(self) -> SettingsSectionMetadata {
        match self {
            Self::Themes => SettingsSectionMetadata {
                title: "Themes",
                icon_asset: Some(SETTINGS_NAV_THEMES_ASSET),
                index: 0,
                debug_selector: "settings-section-themes",
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
            Self::NormaMemory => SettingsSectionMetadata {
                title: "Memory",
                icon_asset: Some("icons/settings-nav-memory.svg"),
                index: 3,
                debug_selector: "settings-section-norma-memory",
            },
            Self::NormaVoice => SettingsSectionMetadata {
                title: "Voice",
                icon_asset: Some("icons/settings-nav-voice.svg"),
                index: 10,
                debug_selector: "settings-section-norma-voice",
            },
            Self::NormaPersona => SettingsSectionMetadata {
                title: "Persona",
                icon_asset: Some("icons/settings-nav-persona.svg"),
                index: 11,
                debug_selector: "settings-section-norma-persona",
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
            Self::Orchestration => SettingsSectionMetadata {
                title: "Orchestration",
                icon_asset: Some("icons/settings-nav-hooks.svg"),
                index: 6,
                debug_selector: "settings-section-orchestration",
            },
            Self::Wallpaper => SettingsSectionMetadata {
                title: "Wallpaper",
                icon_asset: Some("icons/settings-nav-wallpaper.svg"),
                index: 7,
                debug_selector: "settings-section-wallpaper",
            },
            Self::Mobile => SettingsSectionMetadata {
                title: "Mobile Connection",
                icon_asset: Some("icons/settings-nav-mobile.svg"),
                index: 9,
                debug_selector: "settings-section-mobile",
            },
            Self::Performance => SettingsSectionMetadata {
                title: "Performance",
                icon_asset: Some(TOOLBAR_SETTINGS_ASSET),
                index: 8,
                debug_selector: "settings-section-performance",
            },
        }
    }
}

const APPEARANCE_SECTIONS: &[SettingsSection] = &[
    SettingsSection::Themes,
    SettingsSection::Wallpaper,
    SettingsSection::Performance,
];
const INPUT_SECTIONS: &[SettingsSection] = &[SettingsSection::Keyboard];
const AUTOMATION_SECTIONS: &[SettingsSection] = &[
    SettingsSection::Agents,
    SettingsSection::Orchestration,
    SettingsSection::Mobile,
];

const NORMA_SECTIONS: &[SettingsSection] = &[
    SettingsSection::NormaMemory,
    SettingsSection::NormaVoice,
    SettingsSection::NormaPersona,
];

const NAVIGATION_GROUPS: [(&str, &[SettingsSection]); 4] = [
    ("Norma", NORMA_SECTIONS),
    ("Appearance", APPEARANCE_SECTIONS),
    ("Input & Controls", INPUT_SECTIONS),
    ("AI & Automation", AUTOMATION_SECTIONS),
];

const SIDEBAR_GROUP_SPACING: f32 = 23.5;
const NAVIGATION_ITEM_HEIGHT: f32 = 39.0;

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
    let mut groups = div()
        .id("settings-sidebar-scroll")
        .debug_selector(|| "settings-sidebar-scroll".to_string())
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .overflow_x_hidden()
        .flex()
        .flex_col()
        .gap(px(if compact { 14.0 } else { SIDEBAR_GROUP_SPACING }))
        .mt(px(if compact { 14.0 } else { SIDEBAR_GROUP_SPACING }));

    for (group, sections) in NAVIGATION_GROUPS {
        groups = groups.child(sidebar_group(theme, group, sections, active, compact, cx));
    }

    div()
        .id("settings-navigation-sidebar")
        .w(px(sidebar_width))
        .min_w(px(sidebar_width))
        .h_full()
        .flex()
        .flex_col()
        .px(px(if compact { 12.0 } else { 20.0 }))
        .py(px(if compact { 16.0 } else { 28.0 }))
        .border_r_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .bg(rgba(theme.panel_high))
        .child(
            div()
                .mb(px(if compact { 2.0 } else { 6.0 }))
                .relative()
                .top(px(if compact { 0.0 } else { 1.0 }))
                .text_size(px(if compact { 20.0 } else { 24.0 }))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child("Settings"),
        )
        .child(groups)
        .child(settings_brand_footer(theme, compact))
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
                .text_size(px(if title == "Norma" {
                    15.0
                } else if compact {
                    10.0
                } else {
                    11.0
                }))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba(if title == "Norma" {
                    theme.text_primary
                } else {
                    theme.text_secondary
                }))
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
    let selected = section == active;
    let variant = ui::component::button::ButtonCustomVariant::new(cx)
        .color(
            rgba(if selected {
                with_alpha(theme.accent, 0x20)
            } else {
                0x00000000
            })
            .into(),
        )
        .foreground(
            rgba(if selected {
                theme.accent
            } else {
                theme.text_secondary
            })
            .into(),
        )
        .border(rgba(0x00000000).into())
        .hover(rgba(with_alpha(theme.accent, 0x16)).into())
        .active(rgba(with_alpha(theme.accent, 0x32)).into());
    div()
        .id(("settings-section", metadata.index))
        .debug_selector(move || metadata.debug_selector.to_string())
        .h(px(NAVIGATION_ITEM_HEIGHT))
        .when(compact, |item| item.flex_none().min_w(px(110.0)))
        .when(!compact, |item| item.w_full())
        .child(
            Button::new(("settings-destination", metadata.index))
                .custom(variant)
                .w_full()
                .h_full()
                .justify_start()
                .rounded(px(10.0))
                .px(px(12.0))
                .icon(section_icon(theme, section, selected))
                .label(section.title())
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.select_settings_section(section, window, cx);
                })),
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
            theme.accent
        } else {
            theme.text_secondary
        }))
}

#[cfg(test)]
fn canonical_sidebar_group_spacing() -> (f32, f32) {
    (SIDEBAR_GROUP_SPACING, SIDEBAR_GROUP_SPACING)
}

#[cfg(test)]
fn canonical_navigation_item_height() -> f32 {
    NAVIGATION_ITEM_HEIGHT
}

#[cfg(test)]
#[path = "../../tests/unit/settings_navigation_layout_tests.rs"]
mod settings_navigation_layout_tests;

#[cfg(test)]
#[path = "../../tests/unit/settings_navigation_brand_tests.rs"]
mod settings_navigation_brand_tests;

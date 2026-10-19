//! macOS 原生菜单栏的信息架构：先用纯描述表达产品层级，再转换为 GPUI `Menu`。

use crate::app::{
    CheckForUpdates, CloneProject, CopyCardContent, NewBrowser, NewClaudeAgent, NewCodexAgent,
    NewCursorAgent, NewGrokAgent, NewHermesAgent, NewInfCodeAgent, NewKimiAgent, NewOpenCodeAgent,
    NewPiAgent, NewTerminal, NewWindow, NewZCodeAgent, OpenFeatureGuide, OpenPreviewFile,
    OpenProject, QuitNormaOs, ReflowLayout, ToggleMasterLayout, ToggleSettings,
};
use gpui::{Menu, MenuItem, SystemMenuType};

pub(crate) struct MenuSpec {
    pub(crate) label: &'static str,
    pub(crate) items: Vec<MenuItemSpec>,
}

pub(crate) enum MenuItemSpec {
    Separator,
    System {
        label: &'static str,
    },
    Action {
        label: &'static str,
        action: MenuAction,
    },
    Submenu {
        label: &'static str,
        items: Vec<MenuItemSpec>,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum MenuAction {
    CheckForUpdates,
    ToggleSettings,
    QuitNormaOs,
    NewWindow,
    NewTerminal,
    NewBrowser,
    OpenPreviewFile,
    NewClaudeAgent,
    NewCodexAgent,
    NewCursorAgent,
    NewGrokAgent,
    NewOpenCodeAgent,
    NewInfCodeAgent,
    NewKimiAgent,
    NewZCodeAgent,
    NewPiAgent,
    NewHermesAgent,
    OpenProject,
    CloneProject,
    CopyCardContent,
    ReflowLayout,
    ToggleMasterLayout,
    OpenFeatureGuide,
}

/// 返回产品层的菜单树，供单测锁定语义分组，也供 `menu_bar` 统一转换为原生菜单。
pub(crate) fn menu_bar_spec() -> Vec<MenuSpec> {
    menu_bar_spec_for(ui::i18n::Locale::current())
}

pub(crate) fn menu_bar_spec_for(locale: ui::i18n::Locale) -> Vec<MenuSpec> {
    vec![
        MenuSpec {
            label: "Norma OS",
            items: vec![
                action(
                    ui::i18n::text_for(locale, "menu.check_for_updates"),
                    MenuAction::CheckForUpdates,
                ),
                action(
                    ui::i18n::text_for(locale, "menu.settings"),
                    MenuAction::ToggleSettings,
                ),
                MenuItemSpec::Separator,
                MenuItemSpec::System {
                    label: ui::i18n::text_for(locale, "menu.services"),
                },
                MenuItemSpec::Separator,
                action(
                    ui::i18n::text_for(locale, "menu.quit_norma_os"),
                    MenuAction::QuitNormaOs,
                ),
            ],
        },
        MenuSpec {
            label: ui::i18n::text_for(locale, "menu.file"),
            items: vec![
                MenuItemSpec::Submenu {
                    label: ui::i18n::text_for(locale, "menu.new"),
                    items: vec![
                        action(
                            ui::i18n::text_for(locale, "menu.new_window"),
                            MenuAction::NewWindow,
                        ),
                        MenuItemSpec::Separator,
                        action(
                            ui::i18n::text_for(locale, "menu.new_terminal"),
                            MenuAction::NewTerminal,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_browser"),
                            MenuAction::NewBrowser,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.preview_file"),
                            MenuAction::OpenPreviewFile,
                        ),
                        MenuItemSpec::Separator,
                        action(
                            ui::i18n::text_for(locale, "menu.new_claude_agent"),
                            MenuAction::NewClaudeAgent,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_codex_agent"),
                            MenuAction::NewCodexAgent,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_cursor_agent"),
                            MenuAction::NewCursorAgent,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_grok_agent"),
                            MenuAction::NewGrokAgent,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_opencode_agent"),
                            MenuAction::NewOpenCodeAgent,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_infcode_agent"),
                            MenuAction::NewInfCodeAgent,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_kimi_agent"),
                            MenuAction::NewKimiAgent,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_zcode_agent"),
                            MenuAction::NewZCodeAgent,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_pi_agent"),
                            MenuAction::NewPiAgent,
                        ),
                        action(
                            ui::i18n::text_for(locale, "menu.new_hermes_agent"),
                            MenuAction::NewHermesAgent,
                        ),
                    ],
                },
                MenuItemSpec::Separator,
                action(
                    ui::i18n::text_for(locale, "menu.open_project"),
                    MenuAction::OpenProject,
                ),
                action(
                    ui::i18n::text_for(locale, "menu.clone_github_repository"),
                    MenuAction::CloneProject,
                ),
            ],
        },
        MenuSpec {
            label: ui::i18n::text_for(locale, "menu.edit"),
            items: vec![action(
                ui::i18n::text_for(locale, "menu.copy"),
                MenuAction::CopyCardContent,
            )],
        },
        MenuSpec {
            label: ui::i18n::text_for(locale, "menu.layout"),
            items: vec![
                action(
                    ui::i18n::text_for(locale, "menu.reflow_layout"),
                    MenuAction::ReflowLayout,
                ),
                action(
                    ui::i18n::text_for(locale, "menu.toggle_master_layout"),
                    MenuAction::ToggleMasterLayout,
                ),
            ],
        },
        MenuSpec {
            label: ui::i18n::text_for(locale, "menu.help"),
            items: vec![action(
                ui::i18n::text_for(locale, "menu.feature_guide"),
                MenuAction::OpenFeatureGuide,
            )],
        },
    ]
}

/// 将菜单描述转为 GPUI，由 macOS 平台渲染为系统原生菜单。
pub(crate) fn menu_bar() -> Vec<Menu> {
    menu_bar_spec()
        .into_iter()
        .map(|menu| Menu {
            name: menu.label.into(),
            items: menu.items.into_iter().map(menu_item).collect(),
        })
        .collect()
}

fn action(label: &'static str, action: MenuAction) -> MenuItemSpec {
    MenuItemSpec::Action { label, action }
}

fn menu_item(item: MenuItemSpec) -> MenuItem {
    match item {
        MenuItemSpec::Separator => MenuItem::separator(),
        MenuItemSpec::System { label } => MenuItem::os_submenu(label, SystemMenuType::Services),
        MenuItemSpec::Action { label, action } => action_item(label, action),
        MenuItemSpec::Submenu { label, items } => MenuItem::submenu(Menu {
            name: label.into(),
            items: items.into_iter().map(menu_item).collect(),
        }),
    }
}

fn action_item(label: &'static str, action: MenuAction) -> MenuItem {
    match action {
        MenuAction::CheckForUpdates => MenuItem::action(label, CheckForUpdates),
        MenuAction::ToggleSettings => MenuItem::action(label, ToggleSettings),
        MenuAction::QuitNormaOs => MenuItem::action(label, QuitNormaOs),
        MenuAction::NewWindow => MenuItem::action(label, NewWindow),
        MenuAction::NewTerminal => MenuItem::action(label, NewTerminal),
        MenuAction::NewBrowser => MenuItem::action(label, NewBrowser),
        MenuAction::OpenPreviewFile => MenuItem::action(label, OpenPreviewFile),
        MenuAction::NewClaudeAgent => MenuItem::action(label, NewClaudeAgent),
        MenuAction::NewCodexAgent => MenuItem::action(label, NewCodexAgent),
        MenuAction::NewCursorAgent => MenuItem::action(label, NewCursorAgent),
        MenuAction::NewGrokAgent => MenuItem::action(label, NewGrokAgent),
        MenuAction::NewOpenCodeAgent => MenuItem::action(label, NewOpenCodeAgent),
        MenuAction::NewInfCodeAgent => MenuItem::action(label, NewInfCodeAgent),
        MenuAction::NewKimiAgent => MenuItem::action(label, NewKimiAgent),
        MenuAction::NewZCodeAgent => MenuItem::action(label, NewZCodeAgent),
        MenuAction::NewPiAgent => MenuItem::action(label, NewPiAgent),
        MenuAction::NewHermesAgent => MenuItem::action(label, NewHermesAgent),
        MenuAction::OpenProject => MenuItem::action(label, OpenProject),
        MenuAction::CloneProject => MenuItem::action(label, CloneProject),
        MenuAction::CopyCardContent => MenuItem::action(label, CopyCardContent),
        MenuAction::ReflowLayout => MenuItem::action(label, ReflowLayout),
        MenuAction::ToggleMasterLayout => MenuItem::action(label, ToggleMasterLayout),
        MenuAction::OpenFeatureGuide => MenuItem::action(label, OpenFeatureGuide),
    }
}

#[cfg(test)]
#[path = "../tests/unit/menu_bar_language_tests.rs"]
mod language_tests;

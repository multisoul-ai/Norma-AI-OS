//! macOS 原生菜单栏的信息架构：先用纯描述表达产品层级，再转换为 GPUI `Menu`。

use crate::app::{
    CloneProject, CopyCardContent, NewBrowser, NewClaudeAgent, NewCodexAgent, NewCursorAgent,
    NewGrokAgent, NewOpenCodeAgent, NewTerminal, NewWindow, OpenFeatureGuide, OpenPreviewFile,
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
    OpenProject,
    CloneProject,
    CopyCardContent,
    ReflowLayout,
    ToggleMasterLayout,
    OpenFeatureGuide,
}

/// 返回产品层的菜单树，供单测锁定语义分组，也供 `menu_bar` 统一转换为原生菜单。
pub(crate) fn menu_bar_spec() -> Vec<MenuSpec> {
    vec![
        MenuSpec {
            label: "Norma OS",
            items: vec![
                action("Settings…", MenuAction::ToggleSettings),
                MenuItemSpec::Separator,
                MenuItemSpec::System { label: "Services" },
                MenuItemSpec::Separator,
                action("Quit Norma OS", MenuAction::QuitNormaOs),
            ],
        },
        MenuSpec {
            label: "File",
            items: vec![
                MenuItemSpec::Submenu {
                    label: "New",
                    items: vec![
                        action("New Window", MenuAction::NewWindow),
                        MenuItemSpec::Separator,
                        action("New Terminal", MenuAction::NewTerminal),
                        action("New Browser", MenuAction::NewBrowser),
                        action("Preview File…", MenuAction::OpenPreviewFile),
                        MenuItemSpec::Separator,
                        action("New Claude Agent", MenuAction::NewClaudeAgent),
                        action("New Codex Agent", MenuAction::NewCodexAgent),
                        action("New Cursor Agent", MenuAction::NewCursorAgent),
                        action("New Grok Agent", MenuAction::NewGrokAgent),
                        action("New OpenCode Agent", MenuAction::NewOpenCodeAgent),
                    ],
                },
                MenuItemSpec::Separator,
                action("Open Project…", MenuAction::OpenProject),
                action("Clone GitHub Repository…", MenuAction::CloneProject),
            ],
        },
        MenuSpec {
            label: "Edit",
            items: vec![action("Copy", MenuAction::CopyCardContent)],
        },
        MenuSpec {
            label: "Layout",
            items: vec![
                action("Reflow Layout", MenuAction::ReflowLayout),
                action("Toggle Master Layout", MenuAction::ToggleMasterLayout),
            ],
        },
        MenuSpec {
            label: "Help",
            items: vec![action("Feature Guide…", MenuAction::OpenFeatureGuide)],
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
        MenuAction::OpenProject => MenuItem::action(label, OpenProject),
        MenuAction::CloneProject => MenuItem::action(label, CloneProject),
        MenuAction::CopyCardContent => MenuItem::action(label, CopyCardContent),
        MenuAction::ReflowLayout => MenuItem::action(label, ReflowLayout),
        MenuAction::ToggleMasterLayout => MenuItem::action(label, ToggleMasterLayout),
        MenuAction::OpenFeatureGuide => MenuItem::action(label, OpenFeatureGuide),
    }
}

//! Dock command matching. Pure data: no UI, IO, or execution while typing.
use canvas::AgentProfile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockCommand {
    Agent(AgentProfile),
    Terminal,
    Browser,
    Preview,
    Theme(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSuggestion {
    pub title: String,
    pub completion: String,
    /// None means open a category, not execute an operation.
    pub command: Option<DockCommand>,
}

pub struct CommandTheme {
    pub id: &'static str,
    pub title: &'static str,
    pub dark: bool,
}

pub fn suggestions(query: &str, themes: &[CommandTheme]) -> Vec<CommandSuggestion> {
    let query = query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let roots = vec![
        suggestion("新建 Agent", "/new ", None),
        suggestion("新建终端", "/terminal", Some(DockCommand::Terminal)),
        suggestion("新建浏览器", "/browser", Some(DockCommand::Browser)),
        suggestion("预览文件", "/preview", Some(DockCommand::Preview)),
        suggestion("切换主题", "/theme ", None),
    ];
    if query.is_empty() || query == "/" {
        return roots;
    }
    if !query.starts_with('/') {
        return phrase_choices(&query, themes);
    }
    let words: Vec<_> = query.split(' ').collect();
    match words[0] {
        "/new" if words.len() <= 2 => agent_choices(words.get(1).copied().unwrap_or("")),
        "/theme" if words.len() <= 2 => theme_choices(words.get(1).copied().unwrap_or(""), themes),
        _ => roots
            .into_iter()
            .filter(|s| s.completion.trim().starts_with(&query))
            .collect(),
    }
}

fn suggestion(title: &str, completion: &str, command: Option<DockCommand>) -> CommandSuggestion {
    CommandSuggestion {
        title: title.into(),
        completion: completion.into(),
        command,
    }
}

fn agent_choices(filter: &str) -> Vec<CommandSuggestion> {
    [
        AgentProfile::Codex,
        AgentProfile::Claude,
        AgentProfile::OpenCode,
        AgentProfile::Cursor,
        AgentProfile::Grok,
        AgentProfile::Kimi,
        AgentProfile::ZCode,
        AgentProfile::Pi,
        AgentProfile::Hermes,
    ]
    .into_iter()
    .filter(|p| p.display_title().to_lowercase().starts_with(filter))
    .map(|p| {
        suggestion(
            &format!("新建 {}", p.display_title()),
            &format!("/new {}", p.display_title().to_lowercase()),
            Some(DockCommand::Agent(p)),
        )
    })
    .collect()
}

fn theme_choices(filter: &str, themes: &[CommandTheme]) -> Vec<CommandSuggestion> {
    themes
        .iter()
        .filter(|t| match filter {
            "深色" | "暗色" | "dark" => t.dark,
            "浅色" | "light" => !t.dark,
            _ => t.id.starts_with(filter) || t.title.to_lowercase().starts_with(filter),
        })
        .map(|t| {
            suggestion(
                t.title,
                &format!("/theme {}", t.id),
                Some(DockCommand::Theme(t.id.into())),
            )
        })
        .collect()
}

fn strip_prefix<'a>(text: &'a str, prefixes: &[&str]) -> &'a str {
    prefixes
        .iter()
        .find_map(|p| text.strip_prefix(p))
        .unwrap_or(text)
        .trim()
}

fn phrase_choices(query: &str, themes: &[CommandTheme]) -> Vec<CommandSuggestion> {
    // ponytail: 有限的单动作语法；自由描述需另行接入 AI，不能靠包含关键词猜测执行。
    let query = strip_prefix(query, &["请帮我", "帮我", "请"]);
    let theme = strip_prefix(
        query,
        &["切换到", "切换", "设置为", "设置", "更换", "switch to "],
    );
    let theme = theme.strip_suffix("主题").unwrap_or(theme).trim();
    if theme != query || matches!(theme, "深色" | "暗色" | "浅色" | "dark" | "light" | "theme")
    {
        return theme_choices(if theme == "theme" { "" } else { theme }, themes);
    }
    let text = strip_prefix(query, &["创建", "新建", "打开", "create ", "new ", "open "]);
    let text = strip_prefix(text, &["一个", "一张", "an ", "a "]);
    let text = text
        .strip_suffix("卡片")
        .or_else(|| text.strip_suffix(" card"))
        .unwrap_or(text)
        .trim();
    match text {
        "终端" | "terminal" => vec![suggestion(
            "新建终端",
            "/terminal",
            Some(DockCommand::Terminal),
        )],
        "浏览器" | "browser" => vec![suggestion(
            "新建浏览器",
            "/browser",
            Some(DockCommand::Browser),
        )],
        "预览" | "预览文件" | "文件预览" | "preview" => vec![suggestion(
            "预览文件",
            "/preview",
            Some(DockCommand::Preview),
        )],
        "" | "agent" => agent_choices(""),
        _ => agent_choices(text),
    }
}

#[cfg(test)]
#[path = "../tests/unit/command_tests.rs"]
mod tests;

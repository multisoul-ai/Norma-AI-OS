//! Dock command matching. Pure data: no UI, IO, or execution while typing.
use canvas::AgentProfile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockCommand {
    Agent(AgentProfile),
    Terminal,
    Browser,
    Preview,
    Theme(String),
    /// Add an existing repo (absolute path) to the active desktop workspace.
    AddProject(String),
    /// Open the native folder picker to add any other repo.
    BrowseProject,
    /// Open the feature guide.
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandGroup {
    Agent,
    Terminal,
    Project,
    Theme,
    Help,
}

/// A repo that `/add` can attach without a file picker.
pub struct CommandProject {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSuggestion {
    pub group: CommandGroup,
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

pub fn suggestions(
    query: &str,
    themes: &[CommandTheme],
    projects: &[CommandProject],
) -> Vec<CommandSuggestion> {
    let query = query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if query.is_empty() || query == "/" {
        return all_choices(themes);
    }
    if !query.starts_with('/') {
        let phrased = phrase_choices(&query, themes);
        return if phrased.is_empty() {
            leaf_matches(&query, themes, projects)
        } else {
            phrased
        };
    }
    let words: Vec<_> = query.split(' ').collect();
    match words[0] {
        "/new" if words.len() <= 2 => agent_choices(words.get(1).copied().unwrap_or("")),
        "/add" => project_choices(&words[1..].join(" "), projects),
        "/open" if words.len() == 1 => vec![browse_choice()],
        "/theme" if words.len() <= 2 => theme_choices(words.get(1).copied().unwrap_or(""), themes),
        _ => all_choices(themes)
            .into_iter()
            .filter(|s| s.completion.starts_with(&query))
            .collect(),
    }
}

/// Every executable leaf, flat and grouped: Agents, Terminal, Projects, Themes, then Help.
fn all_choices(themes: &[CommandTheme]) -> Vec<CommandSuggestion> {
    let mut all = agent_choices("");
    all.push(terminal_choice());
    all.push(browse_choice());
    all.extend(theme_choices("", themes));
    all.push(help_choice());
    all
}

/// Bare words such as `codex` or `midnight` match a leaf's title or completion.
fn leaf_matches(
    query: &str,
    themes: &[CommandTheme],
    projects: &[CommandProject],
) -> Vec<CommandSuggestion> {
    all_choices(themes)
        .into_iter()
        .chain(project_choices("", projects))
        .filter(|s| s.title.to_lowercase().contains(query) || s.completion.contains(query))
        .collect()
}

fn help_choice() -> CommandSuggestion {
    suggestion(CommandGroup::Help, "帮助", "/help", Some(DockCommand::Help))
}

fn browse_choice() -> CommandSuggestion {
    suggestion(
        CommandGroup::Project,
        "浏览文件夹…",
        "/open",
        Some(DockCommand::BrowseProject),
    )
}

/// Matching repos by name or path, then the folder picker as the fallback.
fn project_choices(filter: &str, projects: &[CommandProject]) -> Vec<CommandSuggestion> {
    projects
        .iter()
        .filter(|p| {
            p.name.to_lowercase().contains(filter) || p.path.to_lowercase().contains(filter)
        })
        .map(|p| {
            suggestion(
                CommandGroup::Project,
                &format!("添加 {}", p.name),
                &format!("/add {}", p.name.to_lowercase()),
                Some(DockCommand::AddProject(p.path.clone())),
            )
        })
        .chain(std::iter::once(browse_choice()))
        .collect()
}

fn terminal_choice() -> CommandSuggestion {
    suggestion(
        CommandGroup::Terminal,
        "新建终端",
        "/terminal",
        Some(DockCommand::Terminal),
    )
}

fn suggestion(
    group: CommandGroup,
    title: &str,
    completion: &str,
    command: Option<DockCommand>,
) -> CommandSuggestion {
    CommandSuggestion {
        group,
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
        AgentProfile::InfCode,
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
            CommandGroup::Agent,
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
                CommandGroup::Theme,
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
        "终端" | "terminal" => vec![terminal_choice()],
        "帮助" | "help" => vec![help_choice()],
        "" | "agent" => agent_choices(""),
        _ => agent_choices(text),
    }
}

#[cfg(test)]
#[path = "../tests/unit/command_tests.rs"]
mod tests;

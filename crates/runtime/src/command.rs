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
    /// `ask <agent> …`：补全后继续输入消息，不直接执行。
    Ask,
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
    let words: Vec<_> = query.split(' ').collect();
    if matches!(words[0], "ask" | "/ask") {
        return if words.len() <= 2 {
            ask_choices(words.get(1).copied().unwrap_or(""))
        } else {
            Vec::new()
        };
    }
    if !query.starts_with('/') {
        let phrased = phrase_choices(&query, themes);
        return if phrased.is_empty() {
            leaf_matches(&query, themes, projects)
        } else {
            phrased
        };
    }
    match words[0] {
        "/new" if words.len() <= 2 => agent_choices(words.get(1).copied().unwrap_or("")),
        "/add" => project_choices(&words[1..].join(" "), projects),
        "/theme" if words.len() <= 2 => theme_choices(words.get(1).copied().unwrap_or(""), themes),
        _ if words.len() == 1 => ranked(all_choices(themes), &words[0][1..]),
        _ => Vec::new(),
    }
}

/// Fuzzy score of `query` against `key` (both lowercase); higher is more similar.
/// Tiers: exact > prefix > word-start > substring > subsequence (3+ chars only).
fn fuzzy_score(query: &str, key: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let len_gap = key.chars().count().saturating_sub(query.chars().count()) as i32;
    if key == query {
        return Some(1000);
    }
    if key.starts_with(query) {
        return Some(800 - len_gap);
    }
    if let Some(pos) = key.find(query) {
        let word_start = key[..pos].ends_with([' ', '-', '/']);
        let base = if word_start { 600 } else { 400 };
        return Some(base - pos as i32 - len_gap);
    }
    if query.chars().count() < 3 {
        return None;
    }
    let mut span_start = None;
    let mut last = 0;
    let mut wanted = query.chars().peekable();
    for (i, c) in key.chars().enumerate() {
        if wanted.peek() == Some(&c) {
            span_start.get_or_insert(i);
            last = i;
            wanted.next();
        }
    }
    if wanted.peek().is_some() {
        return None;
    }
    let span = (last - span_start.unwrap_or(0) + 1) as i32;
    Some(200 - span - len_gap)
}

/// Keep suggestions that fuzzy-match their command name or title, best match first.
fn ranked(items: Vec<CommandSuggestion>, query: &str) -> Vec<CommandSuggestion> {
    let mut scored: Vec<_> = items
        .into_iter()
        .filter_map(|s| {
            let name = s.completion.trim_start_matches('/');
            let name = name
                .strip_prefix("new ")
                .or_else(|| name.strip_prefix("theme "))
                .or_else(|| name.strip_prefix("ask "))
                .unwrap_or(name);
            let full =
                fuzzy_score(query, s.completion.trim_start_matches('/')).filter(|n| *n >= 400);
            let score = [
                fuzzy_score(query, name),
                fuzzy_score(query, &s.title.to_lowercase()),
                full,
            ]
            .into_iter()
            .flatten()
            .max()?;
            Some((score, s))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, s)| s).collect()
}

/// Every executable leaf, flat and grouped: Agents, Terminal, Projects, Themes, then Help.
fn all_choices(themes: &[CommandTheme]) -> Vec<CommandSuggestion> {
    let mut all = agent_choices("");
    all.push(terminal_choice());
    all.push(suggestion(CommandGroup::Ask, "ask", "ask ", None));
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
    let items = all_choices(themes)
        .into_iter()
        .chain(project_choices("", projects))
        .collect();
    ranked(items, query)
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
    let mut scored: Vec<_> = projects
        .iter()
        .filter_map(|p| {
            let score = [
                fuzzy_score(filter, &p.name.to_lowercase()),
                p.path.to_lowercase().contains(filter).then_some(100),
            ]
            .into_iter()
            .flatten()
            .max()?;
            Some((score, p))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .map(|(_, p)| p)
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
    let all = [
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
    .map(|p| {
        suggestion(
            CommandGroup::Agent,
            &format!("新建 {}", p.display_title()),
            &format!("/new {}", p.display_title().to_lowercase()),
            Some(DockCommand::Agent(p)),
        )
    })
    .collect();
    ranked(all, filter)
}

/// `ask codex …` 的 Agent 类型候选；选中只补全到 `ask codex `，等待继续输入消息。
fn ask_choices(filter: &str) -> Vec<CommandSuggestion> {
    let all = agent_choices("")
        .into_iter()
        .filter_map(|choice| match choice.command {
            Some(DockCommand::Agent(profile)) => Some(suggestion(
                CommandGroup::Ask,
                profile.display_title(),
                &format!("ask {} ", profile.display_title().to_lowercase()),
                None,
            )),
            _ => None,
        })
        .collect();
    ranked(all, filter)
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

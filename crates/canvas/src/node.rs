//! 画布节点数据模型（零 gpui）。

use crate::geometry::CanvasRect;
use serde::{Deserialize, Serialize};

pub type NodeId = u64;

/// 普通矩形节点（本切片用于验证拖拽/选中）。color 为 0xRRGGBB。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct RectNode {
    pub color: u32,
}

/// 终端节点的可持久化描述。实际 PTY/session 存在 runtime 层。
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct TermSpec;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum AgentProfile {
    Claude,
    Codex,
    Cursor,
    OpenCode,
}

impl AgentProfile {
    pub fn display_title(self) -> &'static str {
        match self {
            AgentProfile::Claude => "Claude",
            AgentProfile::Codex => "Codex",
            AgentProfile::Cursor => "Cursor",
            AgentProfile::OpenCode => "OpenCode",
        }
    }
}

/// Agent 节点的可持久化描述。实际进程/session 存在 runtime 层。
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentSpec {
    pub profile: AgentProfile,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub definition_id: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum PreviewType {
    Markdown,
    Text,
    Image,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PreviewSpec {
    pub path: String,
    pub explicit_type: Option<PreviewType>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BrowserSpec {
    pub url: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum NodeType {
    Rect,
    Term,
    Agent,
    Preview,
    Browser,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum NodeKind {
    Rect(RectNode),
    Term(TermSpec),
    Agent(AgentSpec),
    Preview(PreviewSpec),
    Browser(BrowserSpec),
}

/// 兼容旧调用方的类型名；新代码优先使用 NodeKind。
pub type CanvasNodeKind = NodeKind;

impl NodeKind {
    pub fn node_type(&self) -> NodeType {
        match self {
            NodeKind::Rect(_) => NodeType::Rect,
            NodeKind::Term(_) => NodeType::Term,
            NodeKind::Agent(_) => NodeType::Agent,
            NodeKind::Preview(_) => NodeType::Preview,
            NodeKind::Browser(_) => NodeType::Browser,
        }
    }

    pub fn display_title(&self) -> String {
        match self {
            NodeKind::Rect(_) => "Node".to_string(),
            NodeKind::Term(_) => "Terminal".to_string(),
            NodeKind::Agent(node) => node
                .alias
                .as_deref()
                .unwrap_or_else(|| node.profile.display_title())
                .to_string(),
            NodeKind::Preview(spec) => preview_title(&spec.path),
            NodeKind::Browser(spec) => browser_title(spec),
        }
    }
}

fn browser_title(spec: &BrowserSpec) -> String {
    spec.title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .map(str::to_string)
        .or_else(|| browser_host(&spec.url))
        .unwrap_or_else(|| "Browser".to_string())
}

fn browser_host(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .trim();
    (!host.is_empty() && !host.chars().any(char::is_whitespace)).then(|| host.to_string())
}

fn preview_title(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Preview")
        .to_string()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CanvasNode {
    pub id: NodeId,
    pub kind: CanvasNodeKind,
    pub bounds: CanvasRect,
    pub z_index: i32,
    pub selected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Agent 节点只保存 profile 描述，并按 profile 暴露显示标题与类型。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude Agent: profile=Claude
    ///   Codex Agent:  profile=Codex
    ///   Cursor Agent: profile=Cursor
    ///   OpenCode Agent: profile=OpenCode
    ///   runtime session 不在 canvas 层构造，因此这里没有 session_id 字段
    ///
    /// 执行过程：
    ///   1. 构造 NodeKind::Agent(Claude)
    ///   2. 构造 NodeKind::Agent(Codex)
    ///   3. 构造 NodeKind::Agent(Cursor)
    ///   4. 构造 NodeKind::Agent(OpenCode)
    ///   5. 调用 node_type / display_title
    ///
    /// 预期结果：
    ///   - 正断言：Claude/Codex/Cursor/OpenCode Agent 类型都是 NodeType::Agent
    ///   - 正断言：标题分别为 "Claude"、"Codex"、"Cursor" 和 "OpenCode"
    ///   - 负断言：Agent 标题不应退化为普通 "Terminal"
    #[test]
    fn test_agent_kind_is_description_only_and_exposes_profile_title() {
        let claude = NodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: None,
            definition_id: None,
        });
        let codex = NodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
            definition_id: None,
        });
        let opencode = NodeKind::Agent(AgentSpec {
            profile: AgentProfile::OpenCode,
            alias: None,
            definition_id: None,
        });
        let cursor = NodeKind::Agent(AgentSpec {
            profile: AgentProfile::Cursor,
            alias: None,
            definition_id: None,
        });

        assert_eq!(
            claude.node_type(),
            NodeType::Agent,
            "Claude Agent 应暴露 NodeType::Agent，供 runtime/app registry dispatch"
        );
        assert_eq!(
            codex.node_type(),
            NodeType::Agent,
            "Codex Agent 应暴露 NodeType::Agent，供 runtime/app registry dispatch"
        );
        assert_eq!(
            opencode.node_type(),
            NodeType::Agent,
            "OpenCode Agent 应暴露 NodeType::Agent，供 runtime/app registry dispatch"
        );
        assert_eq!(
            cursor.node_type(),
            NodeType::Agent,
            "Cursor Agent 应暴露 NodeType::Agent，供 runtime/app registry dispatch"
        );
        assert_eq!(
            claude.display_title(),
            "Claude",
            "Claude Agent 标题应显示为 Claude"
        );
        assert_eq!(
            codex.display_title(),
            "Codex",
            "Codex Agent 标题应显示为 Codex"
        );
        assert_eq!(
            opencode.display_title(),
            "OpenCode",
            "OpenCode Agent 标题应显示为 OpenCode"
        );
        assert_eq!(
            cursor.display_title(),
            "Cursor",
            "Cursor Agent 标题应显示为 Cursor"
        );
        assert_ne!(
            claude.display_title(),
            "Terminal",
            "Claude Agent 不应退化显示为普通 Terminal"
        );
        assert_ne!(
            codex.display_title(),
            "Terminal",
            "Codex Agent 不应退化显示为普通 Terminal"
        );
        assert_ne!(
            opencode.display_title(),
            "Terminal",
            "OpenCode Agent 不应退化显示为普通 Terminal"
        );
        assert_ne!(
            cursor.display_title(),
            "Terminal",
            "Cursor Agent 不应退化显示为普通 Terminal"
        );
    }

    /// Agent alias 是可持久化显示名；旧 snapshot 缺失 alias 时应兼容恢复为 None。
    ///
    /// 数据构造（含关键数值推导）：
    ///   legacy_json = {"Agent":{"profile":"Codex"}}，字段数 1，缺少 alias
    ///   aliased Agent: profile=Codex, alias="Alan"
    ///   display_title 优先 alias，其次 profile title
    ///
    /// 执行过程：
    ///   1. 反序列化 legacy_json 为 NodeKind
    ///   2. 构造 alias=Alan 的 NodeKind
    ///   3. 分别读取 display_title 与 alias 字段
    ///
    /// 预期结果：
    ///   - 正断言：legacy alias 为 None，标题仍是 Codex
    ///   - 正断言：aliased 标题显示 Alan
    ///   - 负断言：aliased 标题不应退回 Codex
    #[test]
    fn test_agent_alias_defaults_for_legacy_snapshot_and_overrides_title() {
        let legacy_json = r#"{"Agent":{"profile":"Codex"}}"#;
        let legacy: NodeKind =
            serde_json::from_str(legacy_json).expect("legacy Agent snapshot 应可恢复");
        let aliased = NodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("Alan".to_string()),
            definition_id: None,
        });

        let NodeKind::Agent(legacy_spec) = &legacy else {
            panic!("legacy json 应恢复为 Agent kind");
        };

        assert_eq!(
            legacy_spec.alias, None,
            "旧 snapshot 缺少 alias 时应默认恢复为 None"
        );
        assert_eq!(
            legacy.display_title(),
            "Codex",
            "legacy Agent 没有 alias 时标题应回退到 profile title"
        );
        assert_eq!(
            aliased.display_title(),
            "Alan",
            "Agent alias 存在时标题应优先显示 alias"
        );
        assert_ne!(
            aliased.display_title(),
            "Codex",
            "带 alias 的 Agent 不应继续显示 profile title"
        );
    }

    /// Term/Rect 节点只暴露节点类型和默认标题，不暴露 runtime session。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Term: TermSpec（空描述，runtime 负责实际 session）
    ///   Rect: color=0x4080ff（颜色不影响 terminal helper）
    ///
    /// 执行过程：
    ///   1. 构造 NodeKind::Term(TermSpec)
    ///   2. 构造 NodeKind::Rect
    ///   3. 分别调用 node_type / display_title
    ///
    /// 预期结果：
    ///   - 正断言：Term 类型是 NodeType::Term，标题为 "Terminal"
    ///   - 正断言：Rect 类型是 NodeType::Rect，标题为 "Node"
    ///   - 负断言：Rect 类型不应是 NodeType::Term
    #[test]
    fn test_node_type_and_titles_for_term_and_rect_nodes() {
        let term = NodeKind::Term(TermSpec);
        let rect = NodeKind::Rect(RectNode { color: 0x4080ff });

        assert_eq!(
            term.node_type(),
            NodeType::Term,
            "Term 节点应暴露 NodeType::Term，供 card registry dispatch"
        );
        assert_eq!(
            term.display_title(),
            "Terminal",
            "Term 节点默认标题应保持 Terminal"
        );
        assert_eq!(
            rect.node_type(),
            NodeType::Rect,
            "Rect 节点应暴露 NodeType::Rect"
        );
        assert_eq!(rect.display_title(), "Node", "Rect 节点默认标题应保持 Node");
        assert_ne!(
            rect.node_type(),
            NodeType::Term,
            "Rect 节点不应伪装成 Term，否则 app 会错误选择 terminal card"
        );
    }

    /// Preview 节点只保存本地文件路径和可选显式类型，并可完整 serde 往返。
    ///
    /// 数据构造（含关键数值推导）：
    ///   path = "/tmp/report.md"（绝对本地路径，内容不进入 canvas 层）
    ///   explicit_type = Some(Markdown)（用户或后续 agent 可显式指定类型）
    ///   snapshot payload 字段数 = 2（path + explicit_type），不包含文件内容字段
    ///
    /// 执行过程：
    ///   1. 构造 NodeKind::Preview(PreviewSpec)
    ///   2. 调用 node_type / display_title
    ///   3. serde_json 序列化后再反序列化
    ///
    /// 预期结果：
    ///   - 正断言：Preview 类型是 NodeType::Preview，标题优先显示文件名 report.md
    ///   - 正断言：serde 往返后 kind 与原始值一致，路径和显式类型仍保留
    ///   - 负断言：序列化 JSON 不应包含文件内容字段，避免快照膨胀
    #[test]
    fn test_preview_kind_preserves_path_type_and_serializes_without_content() {
        let kind = NodeKind::Preview(PreviewSpec {
            path: "/tmp/report.md".to_string(),
            explicit_type: Some(PreviewType::Markdown),
        });

        let json = serde_json::to_string(&kind).expect("Preview kind 应可序列化");
        let restored: NodeKind = serde_json::from_str(&json).expect("Preview kind 应可反序列化");

        assert_eq!(
            kind.node_type(),
            NodeType::Preview,
            "Preview 节点应暴露 NodeType::Preview，供 app card registry dispatch"
        );
        assert_eq!(
            kind.display_title(),
            "report.md",
            "Preview 节点标题应优先显示路径中的文件名"
        );
        assert_eq!(
            restored, kind,
            "Preview kind serde 往返后应完整保留 path 与 explicit_type"
        );
        assert!(
            !json.contains("content"),
            "Preview canvas snapshot 不应包含文件内容副本，实际 JSON: {json}"
        );
    }

    /// Browser 节点只保存网页地址和可选标题，并可完整 serde 往返。
    ///
    /// 数据构造（含关键数值推导）：
    ///   url = "https://example.com/docs"，网页内容不进入 canvas 层
    ///   title = Some("Docs")，标题来自 app 层 WebView 回调
    ///   snapshot payload 字段数 = 2（url + title），不包含 HTML、cookie 或 cache 字段
    ///
    /// 执行过程：
    ///   1. 构造 NodeKind::Browser(BrowserSpec)
    ///   2. 调用 node_type / display_title
    ///   3. serde_json 序列化后再反序列化
    ///
    /// 预期结果：
    ///   - 正断言：Browser 类型是 NodeType::Browser，标题优先显示 "Docs"
    ///   - 正断言：serde 往返后 kind 与原始值一致，url/title 仍保留
    ///   - 负断言：序列化 JSON 不应包含 HTML 内容或 cookie/cache 状态
    #[test]
    fn test_browser_kind_preserves_url_title_and_serializes_without_page_state() {
        let kind = NodeKind::Browser(BrowserSpec {
            url: "https://example.com/docs".to_string(),
            title: Some("Docs".to_string()),
        });

        let json = serde_json::to_string(&kind).expect("Browser kind 应可序列化");
        let restored: NodeKind = serde_json::from_str(&json).expect("Browser kind 应可反序列化");

        assert_eq!(
            kind.node_type(),
            NodeType::Browser,
            "Browser 节点应暴露 NodeType::Browser，供 app card registry dispatch"
        );
        assert_eq!(
            kind.display_title(),
            "Docs",
            "Browser 节点标题应优先显示 WebView 提供的页面标题"
        );
        assert_eq!(
            restored, kind,
            "Browser kind serde 往返后应完整保留 url 与 title"
        );
        assert!(
            !json.contains("html"),
            "Browser canvas snapshot 不应包含 HTML 内容副本，实际 JSON: {json}"
        );
        assert!(
            !json.contains("cookie") && !json.contains("cache"),
            "Browser canvas snapshot 不应包含 cookie/cache 状态，实际 JSON: {json}"
        );
    }

    /// Browser 无标题时用 URL host 做标题，URL 不可解析时回退为 Browser。
    ///
    /// 数据构造（含关键数值推导）：
    ///   titled url = "https://docs.example.com/path"，host = docs.example.com
    ///   invalid url = "not a url"，没有 scheme/host
    ///   title=None，display_title 必须从 URL fallback 推导
    ///
    /// 执行过程：
    ///   1. 构造 title=None 且 URL 有 host 的 Browser kind
    ///   2. 构造 title=None 且 URL 不可解析的 Browser kind
    ///   3. 分别调用 display_title
    ///
    /// 预期结果：
    ///   - 正断言：可解析 URL 标题为 docs.example.com
    ///   - 正断言：不可解析 URL 标题为 Browser
    ///   - 负断言：无标题 Browser 不应退化显示为 Terminal
    #[test]
    fn test_browser_title_falls_back_to_host_then_browser_label() {
        let hosted = NodeKind::Browser(BrowserSpec {
            url: "https://docs.example.com/path".to_string(),
            title: None,
        });
        let invalid = NodeKind::Browser(BrowserSpec {
            url: "not a url".to_string(),
            title: None,
        });

        assert_eq!(
            hosted.display_title(),
            "docs.example.com",
            "Browser 无页面标题时应从 URL host 推导标题"
        );
        assert_eq!(
            invalid.display_title(),
            "Browser",
            "Browser 无法从 URL 推导 host 时应回退为 Browser"
        );
        assert_ne!(
            hosted.display_title(),
            "Terminal",
            "Browser 标题 fallback 不应退化成 Terminal"
        );
    }
}

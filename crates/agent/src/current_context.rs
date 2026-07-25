#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanvasCardKind {
    Rect,
    Terminal,
    Agent,
    Preview,
    Browser,
}

impl CanvasCardKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::Terminal => "terminal",
            Self::Agent => "agent",
            Self::Preview => "preview",
            Self::Browser => "browser",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanvasCardContext {
    pub node_id: u64,
    pub position: usize,
    pub kind: CanvasCardKind,
    pub title: String,
    pub selected: bool,
    pub focused: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentCanvasContext {
    pub project_name: String,
    pub workspace_root: String,
    pub total_cards: usize,
    pub cards: Vec<CanvasCardContext>,
}

pub const MAX_CURRENT_CANVAS_CARDS: usize = 64;
const MAX_CARD_TITLE_CHARS: usize = 80;

pub fn render_current_canvas_context(context: &CurrentCanvasContext) -> String {
    let mut cards = context.cards.iter().collect::<Vec<_>>();
    cards.sort_by_key(|card| (card_priority(card), card.node_id));
    cards.truncate(MAX_CURRENT_CANVAS_CARDS);
    let total_cards = context.total_cards.max(context.cards.len());
    let omitted_cards = total_cards.saturating_sub(cards.len());
    let mut lines = vec![
        "<current-canvas-context>".to_string(),
        format!(
            "project_name={} workspace_root={}",
            quoted(&context.project_name),
            quoted(&context.workspace_root)
        ),
        format!(
            "total_cards={} included_cards={} omitted_cards={}",
            total_cards,
            cards.len(),
            omitted_cards
        ),
    ];
    lines.extend(cards.into_iter().map(|card| {
        format!(
            "node_id={} position={} kind={} title={} selected={} focused={}",
            card.node_id,
            card.position,
            card.kind.as_str(),
            quoted(&truncate_chars(&card.title, MAX_CARD_TITLE_CHARS)),
            card.selected,
            card.focused
        )
    }));
    lines.push("</current-canvas-context>".to_string());
    lines.join("\n")
}

fn card_priority(card: &CanvasCardContext) -> u8 {
    if card.focused {
        0
    } else if card.kind == CanvasCardKind::Agent {
        1
    } else if card.selected {
        2
    } else {
        3
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut characters = value.chars();
    let prefix = characters.by_ref().take(max_chars).collect::<String>();
    if characters.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string to JSON cannot fail")
}

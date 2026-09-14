use super::Notice;
use norma::mobile::ProjectSnapshot;
use serde_json::{json, Value};

pub(super) fn content(project: &ProjectSnapshot, notice: &Notice) -> Value {
    // The shared run/session matcher has already accepted this exact card.
    let card = project
        .cards
        .as_ref()
        .and_then(|cards| cards.iter().find(|card| card.node_id == notice.key.node_id))
        .expect("matched notification card");
    let fallback = match notice.kind {
        "approval" => "有一项操作需要你确认，打开卡片查看。",
        "failed" => "本次任务失败，打开卡片查看具体原因。",
        _ => "本次任务已完成，打开卡片查看完整输出。",
    };
    let title = format!(
        "{} · #{}",
        excerpt(&card.title, 200, &card.profile),
        card.node_id
    );
    json!({
        "project": excerpt(&project.display_name, 160, "未命名项目"),
        "card": title,
        "summary": excerpt(notice.summary.as_deref().unwrap_or(""), 640, fallback),
    })
}

fn excerpt(raw: &str, bytes: usize, fallback: &str) -> String {
    // At most 4096 source scalars, matching task-card previews; UTF-8 budgets also
    // bound APNs JSON escaping: 640+240+160 preview bytes leave room in its 4KiB limit.
    let clean: String = raw
        .chars()
        .take(4096)
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() {
        return fallback.into();
    }
    if clean.len() <= bytes {
        return clean;
    }
    let mut end = bytes - '…'.len_utf8();
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &clean[..end])
}

#[cfg(test)]
#[path = "../../../tests/unit/mobile_push_preview_tests.rs"]
mod tests;

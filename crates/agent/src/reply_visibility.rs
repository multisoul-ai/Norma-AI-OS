pub fn visible_norma_reply(text: &str) -> String {
    let mut visible = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(start) = remaining.find("<think>") {
        visible.push_str(&remaining[..start]);
        let after_start = &remaining[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            return visible.trim().to_string();
        };
        remaining = &after_start[end + "</think>".len()..];
    }
    visible.push_str(remaining);
    visible.trim().to_string()
}

pub(crate) fn finalize_stream_deltas(
    final_visible: &str,
    pending_visible: &str,
    mut pending_deltas: Vec<String>,
) -> Vec<String> {
    if let Some(remaining) = final_visible.strip_prefix(pending_visible) {
        if !remaining.is_empty() {
            pending_deltas.push(remaining.to_string());
        }
        pending_deltas
    } else {
        vec![final_visible.to_string()]
    }
}

pub(crate) fn visible_stream_snapshot(text: &str) -> String {
    let mut visible = String::with_capacity(text.len());
    let mut remaining = text;
    loop {
        let Some(start) = remaining.find("<think>") else {
            let held = longest_tag_prefix_suffix(remaining, "<think>");
            visible.push_str(&remaining[..remaining.len() - held]);
            return visible;
        };
        visible.push_str(&remaining[..start]);
        let after_start = &remaining[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            return visible;
        };
        remaining = &after_start[end + "</think>".len()..];
    }
}

fn longest_tag_prefix_suffix(text: &str, tag: &str) -> usize {
    (1..tag.len())
        .rev()
        .find(|length| text.ends_with(&tag[..*length]))
        .unwrap_or(0)
}

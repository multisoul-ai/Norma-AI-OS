use agent::MemoryKind;

pub(super) fn body_text(markdown: &str) -> String {
    let normalized = markdown.replace("\r\n", "\n");
    normalized
        .strip_prefix("---\n")
        .and_then(|text| text.split_once("\n---\n"))
        .map_or_else(|| normalized.clone(), |(_, body)| body.to_string())
}

pub(super) fn encode(
    name: &str,
    description: &str,
    kind: Option<MemoryKind>,
    modified: Option<&str>,
    body: &str,
) -> String {
    let name = serde_json::to_string(name).expect("text serializes");
    let description = serde_json::to_string(description).expect("text serializes");
    let kind = kind
        .and_then(|kind| serde_json::to_value(kind).ok())
        .and_then(|value| value.as_str().map(str::to_owned));
    format!(
        "---\nname: {name}\ndescription: {description}\n{}{}---\n{body}",
        kind.map(|kind| format!("type: {kind}\n"))
            .unwrap_or_default(),
        modified
            .map(|value| format!(
                "modified: {}\n",
                serde_json::to_string(value).expect("text serializes")
            ))
            .unwrap_or_default()
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/norma_memory_form_tests.rs"]
mod tests;

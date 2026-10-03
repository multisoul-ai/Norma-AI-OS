use super::{MemoryError, MemoryResult};
use agent::{MemoryEntry, MemoryEntryId, MemoryKind, MemorySourceReference, MemorySummary};
use std::collections::BTreeMap;

// Match the reference implementation's per-topic and catalog byte ceilings.
pub const MAX_TOPIC_BYTES: usize = 256_000;
pub const MAX_CATALOG_BYTES: usize = 25_000;

pub(super) fn document(
    id: MemoryEntryId,
    markdown: String,
    sources: Vec<MemorySourceReference>,
) -> MemoryResult<MemoryEntry> {
    if markdown.len() > MAX_TOPIC_BYTES {
        return Err(MemoryError::TooLarge);
    }
    if markdown.trim().is_empty() {
        return Err(MemoryError::InvalidDocument("empty topic".into()));
    }
    if sources.iter().any(|source| !source.is_well_formed()) {
        return Err(MemoryError::InvalidDocument(
            "invalid source reference".into(),
        ));
    }
    let mut summary = MemorySummary {
        name: id.as_str().into(),
        id,
        description: String::new(),
        kind: None,
        modified: None,
    };
    let mut lines = markdown.lines();
    if lines.next() == Some("---") {
        let mut fields = BTreeMap::new();
        let mut closed = false;
        for line in lines.by_ref() {
            if line == "---" {
                closed = true;
                break;
            }
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| invalid_header("expected key: value"))?;
            if !matches!(key, "name" | "description" | "type" | "modified")
                || fields.contains_key(key)
            {
                return Err(invalid_header("unknown or duplicate field"));
            }
            fields.insert(key, scalar(value.trim())?);
        }
        if !closed || lines.collect::<Vec<_>>().join("\n").trim().is_empty() {
            return Err(invalid_header("missing closing delimiter or body"));
        }
        summary.name = fields
            .remove("name")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid_header("name is required"))?;
        summary.description = fields
            .remove("description")
            .ok_or_else(|| invalid_header("description is required"))?;
        summary.kind = match fields.remove("type").as_deref() {
            None => None,
            Some("user") => Some(MemoryKind::User),
            Some("feedback") => Some(MemoryKind::Feedback),
            Some("project") => Some(MemoryKind::Project),
            Some("reference") => Some(MemoryKind::Reference),
            Some(_) => return Err(invalid_header("unsupported type")),
        };
        summary.modified = fields.remove("modified");
    }
    Ok(MemoryEntry {
        summary,
        markdown,
        sources,
    })
}

fn scalar(value: &str) -> MemoryResult<String> {
    let result = if value.starts_with('"') {
        serde_json::from_str::<String>(value)
            .map_err(|_| invalid_header("invalid quoted string"))?
    } else if value.starts_with('\'') {
        value
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .ok_or_else(|| invalid_header("invalid quoted string"))?
            .replace("''", "'")
    } else {
        value.to_string()
    };
    if result.chars().any(char::is_control) {
        return Err(invalid_header("fields must be single-line text"));
    }
    Ok(result)
}

fn invalid_header(reason: &str) -> MemoryError {
    MemoryError::InvalidDocument(reason.into())
}

pub(super) fn catalog(entries: &BTreeMap<MemoryEntryId, MemoryEntry>) -> MemoryResult<String> {
    let mut catalog = "# Norma Memory\n\n".to_string();
    for (id, entry) in entries {
        catalog.push_str(&format!(
            "- [{}](topics/{}.md) — {}\n",
            escape(&entry.summary.name),
            id.as_str(),
            escape(&entry.summary.description)
        ));
    }
    if catalog.len() > MAX_CATALOG_BYTES {
        return Err(MemoryError::TooLarge);
    }
    Ok(catalog)
}

fn escape(value: &str) -> String {
    let mut escaped = String::new();
    for c in value.chars() {
        if matches!(c, '\\' | '[' | ']' | '<' | '>' | '`' | '*' | '_') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

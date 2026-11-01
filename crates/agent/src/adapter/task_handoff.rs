use super::{AdapterCommand, ToolContext};
use crate::AgentInputSource;

const LEGACY_PREAMBLE: &str = concat!(
    "Original user request (verbatim):\n",
    "Use the full request below to preserve this assignment's background, requirements, ",
    "examples and constraints. Do not take over work assigned to other agents.\n",
);
const SEPARATOR: &str = "\n\n---\n\n";

/// Model-written task text may omit background. Carry the actual transcript at
/// the typed dispatch boundary, after validation and without changing retry IDs.
pub(super) fn preserve_doubao_request(command: &mut AdapterCommand, context: &ToolContext) {
    if context.input_source != AgentInputSource::DoubaoVoice || context.user_input.trim().is_empty()
    {
        return;
    }
    let text = match command {
        AdapterCommand::AssignAgentTask { task, .. }
        | AdapterCommand::AssignWorkspaceTask { task, .. }
        | AdapterCommand::CreateAgent {
            task: Some(task), ..
        } => task,
        AdapterCommand::ReplyAgent { message, .. } => message,
        AdapterCommand::HandoffToBrain { request } => request,
        _ => return,
    };
    if text == context.user_input.trim() {
        *text = context.user_input.clone();
        return;
    }
    // Flatten only generated handoff wrappers; never parse or rewrite the user's transcript.
    let original = context.user_input.as_str();
    let protected = text
        .match_indices(original)
        .map(|(start, _)| start..start + original.len())
        .collect::<Vec<_>>();
    let mut sections = Vec::new();
    let mut start = 0;
    for (offset, marker) in text.match_indices(LEGACY_PREAMBLE) {
        if protected
            .iter()
            .any(|range| offset < range.end && offset + marker.len() > range.start)
        {
            continue;
        }
        append_section(&mut sections, &text[start..offset], original);
        start = offset + marker.len();
    }
    append_section(&mut sections, &text[start..], original);
    let mut readable = sections.join(SEPARATOR);
    if !readable.contains(original) {
        if !readable.is_empty() {
            readable.push_str(SEPARATOR);
        }
        readable.push_str(original);
    }
    *text = readable;
}

fn append_section<'a>(sections: &mut Vec<&'a str>, section: &'a str, original: &str) {
    let section = if let Some(offset) = section.find(original) {
        let start = (section.len() - section.trim_start().len()).min(offset);
        let end = section.trim_end().len().max(offset + original.len());
        &section[start..end]
    } else {
        section.trim()
    };
    if section.is_empty() {
        return;
    }
    if let Some(index) = sections
        .iter()
        .position(|existing| existing.trim() == section.trim())
    {
        if section.contains(original) {
            sections[index] = section;
        }
    } else {
        sections.push(section);
    }
}

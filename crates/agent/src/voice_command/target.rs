use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VoiceTargetResolution {
    CanvasAgent { definition_id: i64, node_id: u64 },
    OutsideCanvas { definition_id: i64 },
    Assistant,
    Ambiguous,
    Unspecified,
}

pub fn resolve_voice_target(input: &str, roster: &[AgentRosterEntry]) -> VoiceTargetResolution {
    let mut input = input.trim_start();
    while let Some(rest) = ["请", "帮我", "麻烦", "让", "给", "找", "please ", "ask "]
        .iter()
        .find_map(|prefix| input.strip_prefix(prefix))
    {
        input = rest.trim_start();
    }
    let members = leading_members(input, roster);
    if members.len() > 1 {
        return VoiceTargetResolution::Ambiguous;
    }
    if let Some((member, length)) = members.first() {
        let remainder = input[*length..].trim_start();
        if ["和", "与", "、", "&", "and "].iter().any(|join| {
            remainder
                .strip_prefix(join)
                .is_some_and(|rest| !leading_members(rest.trim_start(), roster).is_empty())
        }) {
            return VoiceTargetResolution::Ambiguous;
        }
        if let Some(node_id) = member.active_node_id {
            return VoiceTargetResolution::CanvasAgent {
                definition_id: member.definition_id,
                node_id,
            };
        }
        return VoiceTargetResolution::OutsideCanvas {
            definition_id: member.definition_id,
        };
    }
    if prefix_length(input, "Norma").is_some() || input.starts_with('你') {
        return VoiceTargetResolution::Assistant;
    }
    VoiceTargetResolution::Unspecified
}

pub(super) fn leading_members<'a>(
    input: &str,
    roster: &'a [AgentRosterEntry],
) -> Vec<(&'a AgentRosterEntry, usize)> {
    let mut members: Vec<_> = roster
        .iter()
        .filter(|member| member.enabled)
        .filter_map(|member| {
            std::iter::once(member.name.as_str())
                .chain(member.speech_alias.as_deref())
                .filter_map(|name| prefix_length(input, name))
                .max()
                .map(|length| (member, length))
        })
        .collect();
    let longest = members
        .iter()
        .map(|(_, length)| *length)
        .max()
        .unwrap_or_default();
    members.retain(|(_, length)| *length == longest);
    members
}

fn prefix_length(input: &str, name: &str) -> Option<usize> {
    let name = name.trim();
    if name.is_empty() || !input.get(..name.len())?.eq_ignore_ascii_case(name) {
        return None;
    }
    let suffix = input.get(name.len()..)?;
    if name.ends_with(|c: char| c.is_ascii_alphanumeric())
        && suffix.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some(name.len())
}

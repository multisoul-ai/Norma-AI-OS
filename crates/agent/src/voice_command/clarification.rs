use super::*;

pub(crate) fn clarification_choices(roster: &[AgentRosterEntry]) -> Vec<String> {
    let mut labels: Vec<_> = roster
        .iter()
        .filter(|m| m.enabled && m.active_node_id.is_some())
        .map(|member| {
            let repeated = roster
                .iter()
                .filter(|m| {
                    m.enabled
                        && m.active_node_id.is_some()
                        && m.name.eq_ignore_ascii_case(&member.name)
                })
                .count()
                > 1;
            if repeated {
                format!(
                    "{} #{}",
                    member.name,
                    member.active_node_id.unwrap_or_default()
                )
            } else {
                member.name.clone()
            }
        })
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

pub(crate) fn is_clarification_attempt(input: &str, roster: &[AgentRosterEntry]) -> bool {
    if target_only_answer(input, roster).is_some() {
        return true;
    }
    let lower = input.to_lowercase();
    input.chars().count() <= 96
        && ![
            "算了",
            "取消",
            "不用了",
            "不做了",
            "cancel",
            "never mind",
            "新建",
            "创建",
            "修复",
            "检查",
            "实现",
            "重构",
            "解释",
            "打开",
            "帮我",
            "create ",
            "fix ",
            "check ",
            "open ",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
}

pub(crate) fn target_only_answer(
    answer: &str,
    roster: &[AgentRosterEntry],
) -> Option<VoiceTargetBinding> {
    let answer = answer.trim().trim_end_matches(['。', '.', '！', '!']);
    let mut answer = answer;
    for prefix in ["交给", "让", "给", "请", "to ", "ask "] {
        answer = answer.strip_prefix(prefix).unwrap_or(answer).trim_start();
    }
    for suffix in ["来处理", "来检查", "来做", "处理", "吧"] {
        answer = answer.strip_suffix(suffix).unwrap_or(answer).trim_end();
    }

    let matches: Vec<_> = roster
        .iter()
        .filter(|member| member.enabled && member.active_node_id.is_some())
        .filter(|member| {
            let id = member.active_node_id.unwrap_or_default();
            member.name.eq_ignore_ascii_case(answer)
                || member
                    .speech_alias
                    .as_deref()
                    .is_some_and(|alias| alias.eq_ignore_ascii_case(answer))
                || [
                    format!("#{id}"),
                    format!("卡片 #{id}"),
                    format!("卡片{id}"),
                    format!("card #{id}"),
                    format!("{} #{id}", member.name),
                ]
                .iter()
                .any(|label| label.eq_ignore_ascii_case(answer))
        })
        .collect();
    let [target] = matches.as_slice() else {
        return None;
    };
    Some(VoiceTargetBinding {
        definition_id: target.definition_id,
        node_id: target.active_node_id,
        name: target.name.clone(),
    })
}

pub(crate) fn validate_clarified_proposals(
    original: &str,
    answer: &str,
    roster: &[AgentRosterEntry],
    mut proposals: Vec<VoiceProposal>,
) -> Result<Vec<VoiceCommit>, VoiceFeedback> {
    let target = target_only_answer(answer, roster).ok_or(VoiceFeedback::TargetMismatch)?;
    for proposal in &mut proposals {
        if matches!(
            proposal.tool.as_str(),
            "assign_agent_task" | "reply_agent" | "cancel_agent_reply"
        ) && (proposal.tool != "cancel_agent_reply" || proposal.arguments.get("agent").is_some())
        {
            proposal.arguments["agent"] = Value::String(target.name.clone());
        }
    }
    let expected = VoiceTargetResolution::CanvasAgent {
        definition_id: target.definition_id,
        node_id: target.node_id.ok_or(VoiceFeedback::MissingSession)?,
    };
    let mut commits = validate_against_target(original, roster, proposals, expected)?;
    for commit in &mut commits {
        commit.input = answer.to_owned();
    }
    Ok(commits)
}

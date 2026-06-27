use crate::{
    PlannedCardKind, PlannedCommand, PlannedTarget, RiskLevel, VoicePlan, VoicePlanningRequest,
};
use norma::protocol::AgentProfile;
#[cfg(test)]
use serde_json::Value;

#[cfg(test)]
pub(crate) fn required_plan_hint_json(request: &VoicePlanningRequest) -> Value {
    required_plan_from_request(request)
        .and_then(|plan| serde_json::to_value(plan).ok())
        .unwrap_or(Value::Null)
}

pub(crate) fn repair_plan_with_required_hint(
    request: &VoicePlanningRequest,
    plan: VoicePlan,
) -> (VoicePlan, bool) {
    let Some(required) = required_plan_from_request(request) else {
        return (plan, false);
    };

    if plan_commands_match(&plan.commands, &required.commands) {
        return (plan, false);
    }

    (required, true)
}

fn required_plan_from_request(request: &VoicePlanningRequest) -> Option<VoicePlan> {
    let transcript = request.transcript.trim();
    let normalized = normalize_transcript(transcript);
    if !has_explicit_create_intent(&normalized) {
        return None;
    }

    let mut commands = Vec::new();

    if mentions_codex(&normalized) {
        commands.push(PlannedCommand::Create {
            kind: PlannedCardKind::Agent {
                profile: AgentProfile::Codex,
            },
        });
    }

    if mentions_claude(&normalized) {
        commands.push(PlannedCommand::Create {
            kind: PlannedCardKind::Agent {
                profile: AgentProfile::Claude,
            },
        });
    }

    let terminal_count = requested_terminal_count(&normalized);
    for _ in 0..terminal_count {
        commands.push(PlannedCommand::Create {
            kind: PlannedCardKind::Terminal,
        });
    }

    if let Some(text) = extract_input_text(transcript) {
        let target_step = most_recent_input_target_step(&commands)?;
        commands.push(PlannedCommand::SendText {
            target: PlannedTarget::PreviousStep { step: target_step },
            text,
            submit: submit_requested(&normalized),
        });
    }

    if commands.is_empty() || commands.len() > request.max_steps {
        return None;
    }

    Some(VoicePlan {
        commands,
        confidence: 0.99,
        risk: RiskLevel::Low,
        requires_confirmation: false,
        reason: Some("deterministic required plan from transcript".to_string()),
    })
}

fn normalize_transcript(transcript: &str) -> String {
    transcript
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace() && !matches!(ch, ',' | '，' | '。' | '.' | ';' | '；'))
        .collect()
}

fn mentions_codex(normalized: &str) -> bool {
    normalized.contains("codex")
}

fn has_explicit_create_intent(normalized: &str) -> bool {
    normalized.contains("打开")
        || normalized.contains("开启")
        || normalized.contains("创建")
        || normalized.contains("新建")
        || normalized.contains("启动")
        || normalized.contains("open")
        || normalized.contains("create")
        || normalized.contains("launch")
        || normalized.contains("start")
}

fn mentions_claude(normalized: &str) -> bool {
    normalized.contains("claude")
}

fn requested_terminal_count(normalized: &str) -> usize {
    if !(normalized.contains("终端")
        || normalized.contains("命令行")
        || normalized.contains("terminal"))
    {
        return 0;
    }

    if normalized.contains("两个终端")
        || normalized.contains("两台终端")
        || normalized.contains("2个终端")
        || normalized.contains("2台终端")
        || normalized.contains("twoterminals")
        || normalized.contains("2terminals")
        || normalized.contains("twoterminal")
        || normalized.contains("2terminal")
    {
        2
    } else {
        1
    }
}

fn extract_input_text(transcript: &str) -> Option<String> {
    let markers = [
        "然后输入",
        "再输入",
        "输入",
        "然后发送",
        "再发送",
        "发送",
        "发给",
        "type",
        "input",
        "send",
    ];
    let lower = transcript.to_ascii_lowercase();

    markers.iter().find_map(|marker| {
        let index = lower.find(marker)?;
        trim_input_text(&transcript[index + marker.len()..])
    })
}

fn trim_input_text(raw: &str) -> Option<String> {
    let mut text = raw
        .trim()
        .trim_matches(|ch: char| matches!(ch, ',' | '，' | '。' | '.' | ':' | '：' | ';' | '；'))
        .trim()
        .to_string();

    for suffix in [
        "不要提交",
        "不执行",
        "并回车",
        "然后回车",
        "回车",
        "提交",
        "执行",
    ] {
        if let Some(stripped) = text.strip_suffix(suffix) {
            text = stripped.trim().to_string();
        }
    }

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn submit_requested(normalized: &str) -> bool {
    if normalized.contains("不要提交") || normalized.contains("不执行") {
        return false;
    }

    normalized.contains("提交") || normalized.contains("执行") || normalized.contains("回车")
}

fn most_recent_input_target_step(commands: &[PlannedCommand]) -> Option<usize> {
    commands
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, command)| match command {
            PlannedCommand::Create {
                kind: PlannedCardKind::Terminal,
            } => Some(index),
            _ => None,
        })
        .or_else(|| {
            commands
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, command)| match command {
                    PlannedCommand::Create {
                        kind:
                            PlannedCardKind::Agent {
                                profile: AgentProfile::Codex | AgentProfile::Claude,
                            },
                    } => Some(index),
                    _ => None,
                })
        })
}

fn plan_commands_match(left: &[PlannedCommand], right: &[PlannedCommand]) -> bool {
    left == right
}

#[cfg(test)]
#[path = "../tests/unit/ollama_repair_tests.rs"]
mod tests;

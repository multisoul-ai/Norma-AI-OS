use crate::{CardRef, VoiceCardKind, VoicePlanningRequest};
use norma::protocol::AgentProfile;
use serde::Serialize;

pub(crate) fn planner_prompt(request: &VoicePlanningRequest) -> Result<String, serde_json::Error> {
    let transcript = serde_json::to_string(&request.transcript)?;
    let targets = serde_json::to_string(&planner_targets(request))?;
    Ok(format!(
        "You are Multisoul's local voice planner.\n\n\
         Return exactly one JSON object. No markdown. No text outside JSON.\n\n\
         CURRENT_TRANSCRIPT:\n{}\n\n\
         TARGETS:\n{}\n\n\
         OUTPUT_CONTRACT:\n{}\n\n\
         RULES:\n{}\n\n\
         EXAMPLES:\n{}",
        transcript,
        targets,
        output_contract_prompt(),
        planning_rules_prompt(),
        examples_prompt()
    ))
}

pub(crate) fn json_repair_prompt(
    request: &VoicePlanningRequest,
    malformed_response: &str,
) -> Result<String, serde_json::Error> {
    let transcript = serde_json::to_string(&request.transcript)?;
    let targets = serde_json::to_string(&planner_targets(request))?;
    let malformed = serde_json::to_string(malformed_response)?;
    Ok(format!(
        "Repair the malformed VoicePlan JSON from Multisoul's local voice planner.\n\n\
         Return exactly one JSON object matching the VoicePlan JSON contract. No prose. No fenced code.\n\n\
         CURRENT_TRANSCRIPT:\n{}\n\n\
         TARGETS:\n{}\n\n\
         MALFORMED_RESPONSE:\n{}\n\n\
         OUTPUT_CONTRACT:\n{}\n\n\
         RULES:\n{}",
        transcript,
        targets,
        malformed,
        output_contract_prompt(),
        repair_rules_prompt()
    ))
}

#[derive(Serialize)]
struct PlannerTargets<'a> {
    focused: Option<u64>,
    selected: &'a [u64],
    cards: Vec<PlannerTargetCard<'a>>,
}

#[derive(Serialize)]
struct PlannerTargetCard<'a> {
    node_id: u64,
    name: &'a str,
    kind: String,
    selected: bool,
    focused: bool,
}

fn planner_targets(request: &VoicePlanningRequest) -> PlannerTargets<'_> {
    PlannerTargets {
        focused: request.focused_node_id,
        selected: &request.selected_node_ids,
        cards: request
            .cards
            .iter()
            .map(|card| PlannerTargetCard {
                node_id: card.node_id,
                name: planner_target_name(card),
                kind: planner_target_kind(&card.kind),
                selected: card.selected,
                focused: card.focused,
            })
            .collect(),
    }
}

fn planner_target_name(card: &CardRef) -> &str {
    card.alias
        .as_deref()
        .filter(|alias| !alias.trim().is_empty())
        .unwrap_or(&card.title)
}

fn planner_target_kind(kind: &VoiceCardKind) -> String {
    match kind {
        VoiceCardKind::Terminal => "terminal".to_string(),
        VoiceCardKind::Agent { profile } => format!("agent:{}", agent_profile_name(*profile)),
        VoiceCardKind::Preview => "preview".to_string(),
        VoiceCardKind::Rect => "rect".to_string(),
    }
}

fn agent_profile_name(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "Claude",
        AgentProfile::Codex => "Codex",
    }
}

fn output_contract_prompt() -> &'static str {
    r#"Return JSON like {"commands":[],"confidence":0.9,"risk":"low","requires_confirmation":false,"reason":"short reason"}.
Allowed commands:
{"type":"create","kind":{"kind":"terminal"}}
{"type":"create","kind":{"kind":"agent","profile":"Codex"}}
{"type":"create","kind":{"kind":"agent","profile":"Claude"}}
{"type":"focus","target":{"type":"node_id","node_id":1}}
{"type":"send_text","target":{"type":"previous_step","step":0},"text":"hello","submit":false}
Allowed targets: node_id, alias, previous_step, focused, selected. Allowed risk: low, medium, high."#
}

fn planning_rules_prompt() -> &'static str {
    "1. Every explicit requested card/entity in CURRENT_TRANSCRIPT must become a create or focus command.\n\
     2. Do not merge multiple requested cards into one command. Create one command for every requested card.\n\
     3. If a count is requested, repeat create commands for that count.\n\
     4. 打开codex, 开启codex, 创建codex, or open/create Codex means create a new agent profile Codex card.\n\
     5. Claude Code, 打开claude code, 开启claude code, or open/create Claude means create a new agent profile Claude card.\n\
     6. Terminal, terminal, 终端, 命令行 mean create terminal.\n\
     7. 输入, 发送, 发给, send, input, type mean send_text.\n\
     8. If send_text happens after newly created cards and no explicit target is named, target the most recently created compatible card with previous_step.\n\
     9. Use TARGETS only for existing-card routing by node_id, alias/name, focused, or selected.\n\
     10. 不要提交 or 不执行 means submit=false. 提交, 执行, or 回车 means submit=true.\n\
     11. target and kind must be single objects, never arrays.\n\
     12. Do not output delete, list, unsupported commands, or natural-language-only plans.\n\
     13. Chinese ASR may insert spaces between every character; ignore those spaces when identifying commands.\n\
     14. If ASR says 合 between requested cards, treat it as 和 when the sentence means and.\n\
     15. If uncertain, set confidence below 0.65."
}

fn repair_rules_prompt() -> &'static str {
    "1. Preserve the user's intent from CURRENT_TRANSCRIPT and TARGETS.\n\
     2. Fix only malformed JSON shape, missing required fields, wrong enum casing, or unsupported wrappers.\n\
     3. Do not add unsupported commands.\n\
     4. Use confidence below 0.65 if the malformed response cannot be repaired safely.\n\
     5. target and kind must be single objects, never arrays."
}

fn examples_prompt() -> &'static str {
    r#"Transcript: "打 开 两 个 终 端 合 一 个 codex"
JSON: {"commands":[{"type":"create","kind":{"kind":"agent","profile":"Codex"}},{"type":"create","kind":{"kind":"terminal"}},{"type":"create","kind":{"kind":"terminal"}}],"confidence":0.96,"risk":"low","requires_confirmation":false,"reason":"create one Codex and two terminals"}

Transcript: "创建一个 codex 输入 hello 不要提交"
JSON: {"commands":[{"type":"create","kind":{"kind":"agent","profile":"Codex"}},{"type":"send_text","target":{"type":"previous_step","step":0},"text":"hello","submit":false}],"confidence":0.92,"risk":"low","requires_confirmation":false,"reason":"create Codex and type hello without submitting"}

Transcript: "给当前选中的终端发送 ls 并回车"
JSON: {"commands":[{"type":"send_text","target":{"type":"selected"},"text":"ls","submit":true}],"confidence":0.9,"risk":"medium","requires_confirmation":true,"reason":"send command to selected terminal"}"#
}

#[cfg(test)]
#[path = "../tests/unit/ollama_prompt_tests.rs"]
mod tests;

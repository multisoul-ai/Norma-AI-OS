use super::*;

pub(super) fn binding(
    input: &str,
    expected: &VoiceTargetResolution,
    roster: &[AgentRosterEntry],
) -> Result<VoiceTargetBinding, VoiceFeedback> {
    let lower = input.to_lowercase();
    if !["取消", "停止", "撤回", "cancel ", "stop "]
        .iter()
        .any(|word| lower.contains(word))
        || [
            "不要取消",
            "别取消",
            "不要停止",
            "别停止",
            "不要撤回",
            "don't cancel",
            "do not cancel",
        ]
        .iter()
        .any(|word| lower.contains(word))
    {
        return Err(VoiceFeedback::InvalidProposal);
    }
    let VoiceTargetResolution::CanvasAgent {
        definition_id,
        node_id,
    } = expected
    else {
        return Err(VoiceFeedback::ClarifyTarget {
            candidates: roster
                .iter()
                .filter(|m| m.enabled && m.active_node_id.is_some())
                .map(|m| m.name.clone())
                .collect(),
        });
    };
    let targets: Vec<_> = roster
        .iter()
        .filter(|m| {
            m.enabled && m.definition_id == *definition_id && m.active_node_id == Some(*node_id)
        })
        .collect();
    let [target] = targets.as_slice() else {
        return Err(VoiceFeedback::TargetMismatch);
    };
    Ok(VoiceTargetBinding {
        definition_id: *definition_id,
        node_id: Some(*node_id),
        name: target.name.clone(),
    })
}

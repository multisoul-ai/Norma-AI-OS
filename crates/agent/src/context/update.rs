use super::*;

/// Validate evidence, not the model's asserted authority. Restriction removal never
/// grants execution permission; existing command authorization still applies.
pub fn validate_state_update(
    input: &RecordedContextEvent,
    update: &ContextStateUpdate,
) -> Result<ContextPayload, &'static str> {
    let ContextPayload::UserInput { text: original } = &input.event.payload else {
        return Err("state_requires_user_input");
    };
    if input.reference != update.input
        || matches!(
            input.event.source.origin,
            ContextOrigin::Brain | ContextOrigin::Host
        )
    {
        return Err("state_source_mismatch");
    }
    match &update.change {
        ContextStateChange::Goal { text, supersedes } => {
            verbatim(original, text)?;
            Ok(ContextPayload::Goal {
                text: text.clone(),
                supersedes: supersedes.clone(),
            })
        }
        ContextStateChange::Constraint {
            key,
            text,
            enforcement,
            supersedes,
        } => {
            verbatim(original, text)?;
            if let ConstraintEnforcement::Enforced { operation, target } = enforcement {
                if !explicit_restriction(original, *operation, target.as_deref()) {
                    return Err("enforcement_not_verified");
                }
            }
            Ok(ContextPayload::Constraint {
                key: key.clone(),
                text: text.clone(),
                enforcement: enforcement.clone(),
                supersedes: supersedes.clone(),
            })
        }
        ContextStateChange::RevokeConstraint { key, supersedes } => {
            // First version intentionally accepts only an unambiguous explicit revocation.
            // Quoted text, discussion, or another key cannot relax an existing restriction.
            if original.trim() != format!("取消限制{key}") {
                return Err("revocation_not_verified");
            }
            Ok(ContextPayload::RevokeConstraint {
                key: key.clone(),
                supersedes: supersedes.clone(),
            })
        }
    }
}

fn verbatim(original: &str, proposed: &str) -> Result<(), &'static str> {
    if proposed.trim().is_empty() || proposed != original.trim() {
        Err("state_requires_verbatim_input")
    } else {
        Ok(())
    }
}

fn explicit_restriction(text: &str, operation: ContextOperation, target: Option<&str>) -> bool {
    let expected = match (operation, target) {
        (ContextOperation::DeleteCard, Some(id))
            if id
                .parse::<u64>()
                .is_ok_and(|value| value > 0 && value.to_string() == id) =>
        {
            format!("不要删除卡片{id}")
        }
        (ContextOperation::DeleteCard, None) => "不要删除卡片".into(),
        (ContextOperation::DispatchTask, None) => "不要派发任务".into(),
        (ContextOperation::SendTerminalInput, None) => "不要向终端输入".into(),
        (ContextOperation::ApproveOperation, None) => "不要批准操作".into(),
        (ContextOperation::AnswerQuestion, None) => "不要代答问题".into(),
        _ => return false,
    };
    text.trim() == expected
}

use super::*;
use norma::mobile::{ApprovalSnapshot, ApprovalTarget};

impl Runtime {
    /// Explicit on-demand read. Provider input is drained before returning actionable requests.
    pub fn mobile_approval_snapshots(
        &mut self,
        node: NodeId,
    ) -> Result<Vec<ApprovalSnapshot>, &'static str> {
        self.drain_mobile_codex();
        self.drain_mobile_claude();
        let card = self
            .mobile_card_snapshots()
            .into_iter()
            .find(|c| c.node_id == node)
            .ok_or("not_found")?;
        if !card.mobile_control
            || card.run_id.is_none()
            || card.session_id.is_none()
            || matches!(
                card.state.as_str(),
                "interrupting" | "done" | "ended" | "failed"
            )
        {
            return Ok(vec![]);
        }
        let mut snapshots = Vec::new();
        if self
            .mobile_runs
            .get(&node)
            .is_some_and(MobileRun::is_native)
        {
            if card.state == "waiting_approval" {
                if let Some(approval) = self.native_approval_snapshot(&card) {
                    return Ok(vec![approval]);
                }
                if let (Some(run_id), Some(session_id), Some(turn_id)) =
                    (card.run_id, card.session_id, card.turn_id)
                {
                    snapshots.push(ApprovalSnapshot {
                        target: ApprovalTarget { node_id: node, profile: card.profile, run_id, session_id, turn_id, request_id: "native-terminal".into() },
                        card_title: card.title, tool: "native_terminal".into(),
                        details: serde_json::json!({"message":"请在电脑的原生 Agent 卡片中处理此确认。"}),
                        decisions: vec![],
                    });
                }
            }
            return Ok(snapshots);
        }
        match card.profile.as_str() {
            "Codex" => {
                for hook in self
                    .mobile_codex_external_approvals(
                        node,
                        card.run_id.as_deref().ok_or("approval_unavailable")?,
                    )
                    .map_err(|_| "approval_unavailable")?
                {
                    snapshots.push(ApprovalSnapshot {
                        target: ApprovalTarget {
                            node_id: node, profile: card.profile.clone(), run_id: card.run_id.clone().ok_or("approval_unavailable")?,
                            session_id: card.session_id.clone().ok_or("approval_unavailable")?, turn_id: hook.turn_id,
                            // Distinct namespace prevents Hook IDs from aliasing Provider request IDs.
                            request_id: norma::mobile::redacted_external_hook_id(&hook.id),
                        },
                        card_title: card.title.clone(), tool: "external_permission_hook".into(),
                        details: serde_json::json!({"message":"等待电脑上的外部审批，请在电脑上处理。","source":hook.source}),
                        decisions: vec![],
                    });
                }
                for approval in self
                    .mobile_codex_approvals(node)
                    .map_err(|_| "approval_unavailable")?
                {
                    let decisions = [ApprovalDecision::Allow, ApprovalDecision::Deny]
                        .into_iter()
                        .filter(|decision| approval.response(*decision).is_ok())
                        .collect();
                    snapshots.push(ApprovalSnapshot {
                        target: ApprovalTarget {
                            node_id: node,
                            profile: card.profile.clone(),
                            run_id: approval.run_id,
                            session_id: approval.thread_id,
                            turn_id: approval.turn_id,
                            request_id: approval.id,
                        },
                        card_title: card.title.clone(),
                        tool: approval.method,
                        details: approval.params,
                        decisions,
                    });
                }
            }
            "Claude" => {
                for hook in self.mobile_approvals(node) {
                    let request = hook.request();
                    let event = &request.event;
                    snapshots.push(ApprovalSnapshot {
                        target: ApprovalTarget {
                            node_id: node,
                            profile: card.profile.clone(),
                            run_id: request.run_id.clone(),
                            session_id: event.session_id.clone().ok_or("approval_unavailable")?,
                            turn_id: event.turn_id.clone().ok_or("approval_unavailable")?,
                            request_id: hook.id().into(),
                        },
                        card_title: card.title.clone(),
                        tool: event.tool_name.clone().ok_or("approval_unavailable")?,
                        details: event
                            .raw_json
                            .get("tool_input")
                            .cloned()
                            .ok_or("approval_unavailable")?,
                        decisions: vec![ApprovalDecision::Allow, ApprovalDecision::Deny],
                    });
                }
            }
            _ => {}
        }
        if snapshots.iter().any(|s| !s.target.valid()) {
            return Err("approval_unavailable");
        }
        Ok(snapshots)
    }

    /// This reserves one choice; the caller must separately observe the durable transport audit.
    pub fn choose_mobile_approval(
        &mut self,
        expected: &ApprovalTarget,
        decision: ApprovalDecision,
    ) -> Result<(), &'static str> {
        if !expected.valid() {
            return Err("invalid_request");
        }
        let current = self
            .mobile_approval_snapshots(expected.node_id)?
            .into_iter()
            .find(|s| s.target == *expected)
            .ok_or("approval_changed")?;
        if !current.decisions.contains(&decision) {
            return Err("decision_unavailable");
        }
        match expected.profile.as_str() {
            "Codex" => self
                .decide_mobile_codex_approval(
                    expected.node_id,
                    &expected.run_id,
                    &expected.request_id,
                    decision,
                )
                .map_err(|_| "approval_changed"),
            "Claude"
                if self.decide_mobile_approval(
                    expected.node_id,
                    &expected.run_id,
                    &expected.request_id,
                    decision,
                ) =>
            {
                Ok(())
            }
            _ => Err("approval_changed"),
        }
    }
}

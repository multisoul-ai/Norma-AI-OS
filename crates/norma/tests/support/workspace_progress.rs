use norma::workspace_progress::*;

pub fn query() -> ProgressQuery {
    ProgressQuery {
        queried_at_unix_ms: 1_800_000_000_000,
        timezone: "Asia/Shanghai".into(),
        recent_window: None,
        project_id: None,
    }
}
pub fn facts() -> AuthorizedProgressFacts {
    let mut data = AuthorizedProgressFacts::empty(
        ProgressScope {
            workspace_id: "space-a".into(),
            authorized_area_ids: vec!["area-a".into()],
            authorization_revision: 4,
            whole_workspace: true,
        },
        7,
    );
    data.areas.push(ProgressArea {
        workspace_id: "space-a".into(),
        area_id: "area-a".into(),
        project_id: "project-a".into(),
        project_name: "API".into(),
        presence: AreaPresence::Unloaded,
    });
    data.cards.push(ProgressCardFact {
        card: card(),
        name: "Betty".into(),
        enabled: false,
        retained: true,
        observed_at_unix_ms: 10,
    });
    data
}
pub fn card() -> ProgressCardRef {
    ProgressCardRef {
        workspace_id: "space-a".into(),
        area_id: "area-a".into(),
        node_id: 1,
        card_instance_id: "card-original".into(),
    }
}
pub fn evidence(source: EvidenceSource, time: u64) -> ProgressEvidence {
    ProgressEvidence {
        source,
        kind: EvidenceKind::Transition,
        observed_at_unix_ms: time,
        source_sequence: time,
        freshness: FactFreshness::Persisted,
        quality: vec![],
    }
}
pub fn task(reference: &str, phase: TaskPhase, time: u64) -> TaskProgressFact {
    TaskProgressFact {
        card: card(),
        task_ref: reference.into(),
        logical_task_ref: None,
        attempt_sequence: 0,
        kind: TaskFactKind::LogicalTask,
        name_at_observation: "Alan".into(),
        created_at_unix_ms: 1,
        title: "修复登录".into(),
        phase,
        execution: Some(ProgressExecutionRef {
            runtime_instance_id: "runtime-a".into(),
            run_id: "run-a".into(),
            session_id: "session-a".into(),
            turn_id: "turn-a".into(),
        }),
        evidence: evidence(EvidenceSource::WorkspaceLedger, time),
        summary: None,
        waiting_reason: None,
        steps: None,
        needs_review: false,
        terminal_at_unix_ms: None,
        result_ref: None,
    }
}

pub fn activity(turn: &str, phase: TaskPhase, time: u64) -> CardActivityFact {
    let task = task("unused", phase, time);
    let mut execution = task.execution.expect("fixture包含精确execution");
    execution.turn_id = turn.into();
    CardActivityFact {
        card: task.card,
        name_at_observation: task.name_at_observation,
        execution,
        linked_task_ref: None,
        linked_attempt_sequence: None,
        phase,
        title: task.title,
        summary: task.summary,
        waiting_reason: None,
        evidence: evidence(EvidenceSource::TurnOutcome, time),
        steps: None,
        needs_review: false,
        terminal_at_unix_ms: None,
        result_ref: None,
    }
}

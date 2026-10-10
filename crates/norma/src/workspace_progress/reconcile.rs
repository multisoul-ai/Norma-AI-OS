use super::display::excerpt;
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn summarize_current(
    logical: String,
    tasks: Vec<TaskProgressFact>,
    cards: &BTreeMap<facts::CardKey, ProgressCardFact>,
    window: ProgressWindow,
) -> TaskProgressSummary {
    let attempt = tasks
        .iter()
        .map(|task| task.attempt_sequence)
        .max()
        .expect("group is nonempty");
    let current: Vec<_> = tasks
        .iter()
        .filter(|task| task.attempt_sequence == attempt)
        .collect();
    let terminals: Vec<_> = current
        .iter()
        .copied()
        .filter(|task| task.phase.is_terminal() && task.evidence.kind == EvidenceKind::Transition)
        .collect();
    let candidates = if terminals.is_empty() {
        &current
    } else {
        &terminals
    };
    let task = latest_by_sources(candidates);
    let conflict = terminals
        .iter()
        .map(|t| t.phase)
        .collect::<BTreeSet<_>>()
        .len()
        > 1;
    let unproven = task.evidence.kind == EvidenceKind::DisplayOnly
        || (task.phase.is_terminal() && task.evidence.kind != EvidenceKind::Transition);
    let phase = if conflict || unproven {
        TaskPhase::Unknown
    } else {
        task.phase
    };
    let mut quality: BTreeSet<_> = task.evidence.quality.iter().copied().collect();
    if conflict {
        quality.insert(ProgressQuality::ConflictingEvidence);
    }
    if phase == TaskPhase::Unknown {
        quality.insert(ProgressQuality::OutcomeUnknown);
    }
    if task.evidence.freshness == FactFreshness::NeedsVerification {
        quality.insert(ProgressQuality::Stale);
    }
    let steps = task
        .steps
        .clone()
        .filter(|steps| {
            let valid = !conflict
                && steps.complete_plan
                && steps.total > 0
                && steps.completed <= steps.total;
            if !valid {
                quality.insert(ProgressQuality::IncompletePlan);
            }
            valid
        })
        .map(|mut steps| {
            steps.current_step = steps.current_step.map(|s| excerpt(&s, 120));
            steps
        });
    let terminal_at = phase.is_terminal().then_some(
        task.terminal_at_unix_ms
            .unwrap_or(task.evidence.observed_at_unix_ms),
    );
    let in_recent_window =
        terminal_at.is_some_and(|time| time >= window.start_unix_ms && time <= window.end_unix_ms);
    let needs_review = current.iter().any(|t| t.needs_review);
    let requires_attention = needs_review
        || matches!(
            phase,
            TaskPhase::Failed
                | TaskPhase::Interrupted
                | TaskPhase::Unknown
                | TaskPhase::WaitingApproval
                | TaskPhase::WaitingInput
        )
        || quality.iter().any(|flag| {
            matches!(
                flag,
                ProgressQuality::OutcomeUnknown
                    | ProgressQuality::Stale
                    | ProgressQuality::ConflictingEvidence
                    | ProgressQuality::UnresolvedLink
            )
        });
    let task_refs = tasks
        .iter()
        .map(|task| task.task_ref.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let observations = tasks
        .iter()
        .map(|task| task.evidence.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    TaskProgressSummary {
        card: task.card.clone(),
        logical_task_ref: logical,
        task_refs,
        attention_history: vec![],
        attempt_sequence: attempt,
        name_at_observation: excerpt(&task.name_at_observation, 80),
        current_role_name: cards
            .get(&task.card.key())
            .filter(|c| c.retained)
            .map(|c| c.name.clone()),
        title: excerpt(&task.title, 120),
        phase,
        evidence: task.evidence.clone(),
        observations,
        quality: quality.into_iter().collect(),
        summary: task.summary.as_ref().map(|s| excerpt(s, 600)),
        waiting_reason: task.waiting_reason.as_ref().map(|s| excerpt(s, 240)),
        steps,
        needs_review,
        terminal_at_unix_ms: terminal_at,
        in_recent_window,
        requires_attention,
        include_in_default_report: !phase.is_terminal() || requires_attention || in_recent_window,
        target: ProgressResultTarget {
            card: task.card.clone(),
            task_ref: Some(task.task_ref.clone()),
            execution: (!conflict).then(|| task.execution.clone()).flatten(),
            result_ref: phase
                .is_terminal()
                .then(|| task.result_ref.clone())
                .flatten(),
        },
    }
}
fn latest_by_sources<'a>(facts: &[&'a TaskProgressFact]) -> &'a TaskProgressFact {
    let mut latest: BTreeMap<(EvidenceSource, Option<ProgressExecutionRef>), &TaskProgressFact> =
        BTreeMap::new();
    for fact in facts {
        let key = (fact.evidence.source, fact.execution.clone());
        let version =
            |t: &TaskProgressFact| (t.evidence.source_sequence, t.evidence.observed_at_unix_ms);
        let replace = latest.get(&key).is_none_or(|old| {
            version(fact) > version(old)
                || (version(fact) == version(old) && compare_display(fact, old).is_gt())
        });
        if replace {
            latest.insert(key, fact);
        }
    }
    latest
        .into_values()
        .max_by(|a, b| {
            a.evidence
                .observed_at_unix_ms
                .cmp(&b.evidence.observed_at_unix_ms)
                .then_with(|| a.evidence.source.cmp(&b.evidence.source))
                .then_with(|| compare_display(a, b))
        })
        .expect("current attempt is nonempty")
}
fn compare_display(a: &TaskProgressFact, b: &TaskProgressFact) -> std::cmp::Ordering {
    a.task_ref
        .cmp(&b.task_ref)
        .then_with(|| a.execution.cmp(&b.execution))
        .then_with(|| a.phase.cmp(&b.phase))
        .then_with(|| a.evidence.cmp(&b.evidence))
        .then_with(|| a.title.cmp(&b.title))
        .then_with(|| a.summary.cmp(&b.summary))
        .then_with(|| a.waiting_reason.cmp(&b.waiting_reason))
        .then_with(|| a.result_ref.cmp(&b.result_ref))
}

pub(super) fn recent_task_outcome(
    logical: &str,
    tasks: &[TaskProgressFact],
    cards: &BTreeMap<facts::CardKey, ProgressCardFact>,
    window: ProgressWindow,
) -> Option<ProgressRecentOutcome> {
    let mut attempts: BTreeMap<u64, Vec<TaskProgressFact>> = BTreeMap::new();
    for task in tasks {
        attempts
            .entry(task.attempt_sequence)
            .or_default()
            .push(task.clone());
    }
    attempts
        .into_values()
        .map(|attempt| summarize(logical.into(), attempt, cards, window))
        .filter(|summary| summary.in_recent_window)
        .max_by_key(|summary| (summary.terminal_at_unix_ms, summary.attempt_sequence))
        .and_then(|summary| outcome(&summary, ProgressOutcomeKind::LogicalTask))
}
pub(super) fn outcome(
    summary: &TaskProgressSummary,
    kind: ProgressOutcomeKind,
) -> Option<ProgressRecentOutcome> {
    let time = summary
        .terminal_at_unix_ms
        .filter(|_| summary.in_recent_window)?;
    Some(ProgressRecentOutcome {
        kind,
        logical_task_ref: (kind == ProgressOutcomeKind::LogicalTask)
            .then(|| summary.logical_task_ref.clone()),
        title: summary.title.clone(),
        summary: summary.summary.clone(),
        phase: summary.phase,
        terminal_at_unix_ms: time,
        evidence: summary.evidence.clone(),
        needs_review: summary.needs_review,
        target: summary.target.clone(),
    })
}
pub(super) fn summarize_activity(
    activities: Vec<CardActivityFact>,
    cards: &BTreeMap<facts::CardKey, ProgressCardFact>,
    window: ProgressWindow,
) -> (ActivityProgressSummary, Option<ProgressRecentOutcome>) {
    let execution = activities[0].execution.clone();
    let unresolved = activities
        .iter()
        .any(|a| a.linked_task_ref.is_some() || a.linked_attempt_sequence.is_some());
    let facts = activities
        .into_iter()
        .map(|a| TaskProgressFact {
            card: a.card,
            task_ref: "direct-turn".into(),
            logical_task_ref: None,
            attempt_sequence: 0,
            kind: TaskFactKind::LogicalTask,
            name_at_observation: a.name_at_observation,
            created_at_unix_ms: 0,
            title: a.title,
            phase: a.phase,
            execution: Some(a.execution),
            evidence: a.evidence,
            summary: a.summary,
            waiting_reason: a.waiting_reason,
            steps: a.steps,
            needs_review: a.needs_review,
            terminal_at_unix_ms: a.terminal_at_unix_ms,
            result_ref: a.result_ref,
        })
        .collect();
    let mut summary = summarize("direct-turn".into(), facts, cards, window);
    summary.target.task_ref = None;
    if unresolved && !summary.quality.contains(&ProgressQuality::UnresolvedLink) {
        summary.quality.push(ProgressQuality::UnresolvedLink);
        summary.requires_attention = true;
        summary.include_in_default_report = true;
    }
    let result = outcome(&summary, ProgressOutcomeKind::DirectActivity);
    (
        ActivityProgressSummary {
            card: summary.card,
            name_at_observation: summary.name_at_observation,
            current_role_name: summary.current_role_name,
            execution,
            phase: summary.phase,
            evidence: summary.evidence,
            observations: summary.observations,
            quality: summary.quality,
            title: summary.title,
            summary: summary.summary,
            waiting_reason: summary.waiting_reason,
            steps: summary.steps,
            needs_review: summary.needs_review,
            terminal_at_unix_ms: summary.terminal_at_unix_ms,
            in_recent_window: summary.in_recent_window,
            requires_attention: summary.requires_attention,
            include_in_default_report: summary.include_in_default_report,
            target: summary.target,
        },
        result,
    )
}

pub(super) fn summarize(
    logical: String,
    tasks: Vec<TaskProgressFact>,
    cards: &BTreeMap<facts::CardKey, ProgressCardFact>,
    window: ProgressWindow,
) -> TaskProgressSummary {
    let mut summary = summarize_current(logical.clone(), tasks.clone(), cards, window);
    let mut older: BTreeMap<u64, Vec<TaskProgressFact>> = BTreeMap::new();
    for task in tasks
        .into_iter()
        .filter(|t| t.attempt_sequence < summary.attempt_sequence)
    {
        older.entry(task.attempt_sequence).or_default().push(task);
    }
    for attempt in older.into_values() {
        let previous = summarize_current(logical.clone(), attempt, cards, window);
        let abnormal = matches!(
            previous.phase,
            TaskPhase::Failed | TaskPhase::Interrupted | TaskPhase::Unknown
        ) || previous.needs_review
            || previous.quality.contains(&ProgressQuality::OutcomeUnknown);
        if !abnormal {
            continue;
        }
        for quality in &previous.quality {
            if matches!(
                quality,
                ProgressQuality::OutcomeUnknown | ProgressQuality::ConflictingEvidence
            ) && !summary.quality.contains(quality)
            {
                summary.quality.push(*quality);
            }
        }
        summary.attention_history.push(ProgressAttemptAttention {
            attempt_sequence: previous.attempt_sequence,
            task_refs: previous.task_refs,
            phase: previous.phase,
            evidence: previous.evidence,
            quality: previous.quality,
            summary: previous.summary,
            waiting_reason: previous.waiting_reason,
            needs_review: previous.needs_review,
            target: previous.target,
        });
    }
    summary.quality.sort();
    if !summary.attention_history.is_empty() {
        summary.requires_attention = true;
        summary.include_in_default_report = true;
    }
    summary
}

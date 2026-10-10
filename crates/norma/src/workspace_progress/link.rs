use super::*;
use std::collections::{BTreeMap, BTreeSet};
pub(super) type TaskKey = (facts::CardKey, String);
type TurnKey = (facts::CardKey, ProgressExecutionRef);

/// A proven association applies to every duplicate source for that exact persistent card/turn.
pub(super) fn link_turns(
    groups: &mut BTreeMap<TaskKey, Vec<TaskProgressFact>>,
    activities: Vec<CardActivityFact>,
) -> Result<BTreeMap<TurnKey, Vec<CardActivityFact>>, ProgressError> {
    let mut links = BTreeMap::new();
    for (key, tasks) in groups.iter() {
        for task in tasks {
            for reference in [&task.task_ref, &key.1] {
                let link = (task.card.key(), reference.clone(), task.attempt_sequence);
                if links
                    .insert(link, key.clone())
                    .is_some_and(|old| old != *key)
                {
                    return Err(ProgressError::InvalidFacts);
                }
            }
        }
    }
    let mut turns: BTreeMap<TurnKey, Vec<CardActivityFact>> = BTreeMap::new();
    for activity in activities {
        turns
            .entry((activity.card.key(), activity.execution.clone()))
            .or_default()
            .push(activity);
    }
    let mut direct = BTreeMap::new();
    for (turn, mut sources) in turns {
        let mut proven = BTreeSet::new();
        let mut unresolved = false;
        for activity in &sources {
            match activity
                .linked_task_ref
                .as_ref()
                .zip(activity.linked_attempt_sequence)
            {
                Some((reference, attempt)) => {
                    if let Some(key) = links.get(&(activity.card.key(), reference.clone(), attempt))
                    {
                        proven.insert((key.clone(), attempt));
                    } else {
                        unresolved = true;
                    }
                }
                None => {
                    unresolved |= activity.linked_task_ref.is_some()
                        || activity.linked_attempt_sequence.is_some()
                }
            }
        }
        if proven.len() == 1 && !unresolved {
            let (key, attempt) = proven.into_iter().next().expect("one proven link");
            let tasks = groups
                .get_mut(&key)
                .expect("link points to an existing group");
            let base = tasks
                .iter()
                .find(|task| task.attempt_sequence == attempt)
                .expect("link names a known attempt")
                .clone();
            for activity in sources {
                let mut task = base.clone();
                task.card = activity.card;
                task.name_at_observation = activity.name_at_observation;
                task.phase = activity.phase;
                task.execution = Some(activity.execution);
                task.evidence = activity.evidence;
                task.summary = activity.summary;
                task.waiting_reason = activity.waiting_reason;
                task.steps = activity.steps;
                task.needs_review = activity.needs_review;
                task.terminal_at_unix_ms = activity.terminal_at_unix_ms;
                task.result_ref = activity.result_ref;
                tasks.push(task);
            }
        } else {
            if unresolved || proven.len() > 1 {
                for source in &mut sources {
                    if !source
                        .evidence
                        .quality
                        .contains(&ProgressQuality::UnresolvedLink)
                    {
                        source
                            .evidence
                            .quality
                            .push(ProgressQuality::UnresolvedLink);
                    }
                    if !proven.is_empty()
                        && !source
                            .evidence
                            .quality
                            .contains(&ProgressQuality::ConflictingEvidence)
                    {
                        source
                            .evidence
                            .quality
                            .push(ProgressQuality::ConflictingEvidence);
                    }
                }
            }
            direct.insert(turn, sources);
        }
    }
    Ok(direct)
}

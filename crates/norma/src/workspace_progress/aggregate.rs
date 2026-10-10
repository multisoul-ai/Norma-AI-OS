use super::display::excerpt;
use super::link::{link_turns, TaskKey};
use super::reconcile::{recent_task_outcome, summarize, summarize_activity};
use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// Aggregates the complete authorized facts before adapters bound any display details.
pub fn aggregate_workspace_progress(
    query: ProgressQuery,
    facts: AuthorizedProgressFacts,
) -> Result<WorkspaceProgressSnapshot, ProgressError> {
    if facts.scope.workspace_id.is_empty() || query.timezone.is_empty() {
        return Err(ProgressError::InvalidScope);
    }
    let recent_window = query.recent_window.unwrap_or(ProgressWindow {
        start_unix_ms: query.queried_at_unix_ms.saturating_sub(86_400_000),
        end_unix_ms: query.queried_at_unix_ms,
    });
    if recent_window.start_unix_ms > recent_window.end_unix_ms
        || recent_window.end_unix_ms > query.queried_at_unix_ms
    {
        return Err(ProgressError::InvalidWindow);
    }
    let authorized: BTreeSet<_> = facts.scope.authorized_area_ids.iter().cloned().collect();
    let known_areas: BTreeSet<_> = facts
        .areas
        .iter()
        .filter(|area| {
            area.workspace_id == facts.scope.workspace_id && authorized.contains(&area.area_id)
        })
        .map(|area| area.area_id.clone())
        .collect();
    let mut areas: BTreeMap<_, _> = facts
        .areas
        .into_iter()
        .filter(|area| {
            area.workspace_id == facts.scope.workspace_id
                && authorized.contains(&area.area_id)
                && query
                    .project_id
                    .as_ref()
                    .is_none_or(|id| *id == area.project_id)
        })
        .map(|mut area| {
            area.project_name = excerpt(&area.project_name, 120);
            (area.area_id.clone(), area)
        })
        .collect();
    // Missing project attribution cannot erase other authorized facts. Never guess a project.
    if query.project_id.is_none() {
        for area in authorized.difference(&known_areas) {
            areas.insert(
                area.clone(),
                ProgressArea {
                    workspace_id: facts.scope.workspace_id.clone(),
                    area_id: area.clone(),
                    project_id: String::new(),
                    project_name: "项目归属待核对".into(),
                    presence: AreaPresence::Unknown,
                },
            );
        }
    }
    let expected: BTreeSet<_> = areas
        .keys()
        .cloned()
        .chain(authorized.difference(&known_areas).cloned())
        .collect();
    let coverage = coverage::scoped_coverage(
        &facts.scope.workspace_id,
        &expected,
        &known_areas,
        facts.coverage,
    );
    let coverage_complete = coverage.iter().all(|c| c.state == CoverageState::Complete);
    let unresolved_history =
        history::scoped_history(&facts.scope.workspace_id, &areas, facts.unresolved_history)?;
    let readable = |card: &ProgressCardRef| {
        card.workspace_id == facts.scope.workspace_id && areas.contains_key(&card.area_id)
    };
    let mut cards = BTreeMap::new();
    for mut card in facts.cards.into_iter().filter(|card| readable(&card.card)) {
        card.name = excerpt(&card.name, 80);
        let old: Option<&ProgressCardFact> = cards.get(&card.card.key());
        if old.is_none_or(|old| old.observed_at_unix_ms <= card.observed_at_unix_ms) {
            cards.insert(card.card.key(), card);
        }
    }
    let mut groups: BTreeMap<TaskKey, Vec<TaskProgressFact>> = BTreeMap::new();
    for task in facts
        .tasks
        .into_iter()
        .filter(|task| readable(&task.card) && task.kind == TaskFactKind::LogicalTask)
    {
        if task.task_ref.is_empty() || task.card.card_instance_id.is_empty() {
            return Err(ProgressError::InvalidFacts);
        }
        let logical = task
            .logical_task_ref
            .clone()
            .unwrap_or_else(|| task.task_ref.clone());
        groups
            .entry((task.card.key(), logical))
            .or_default()
            .push(task);
    }
    let activities = facts
        .activities
        .into_iter()
        .filter(|a| readable(&a.card))
        .collect();
    let activities = link_turns(&mut groups, activities)?;
    let mut projects: BTreeMap<String, ProgressProjectSummary> = BTreeMap::new();
    for area in areas.into_values() {
        projects
            .entry(area.project_id.clone())
            .or_insert_with(|| ProgressProjectSummary {
                project_identity_known: !area.project_id.is_empty(),
                project_id: area.project_id.clone(),
                name: area.project_name.clone(),
                areas: vec![],
                roles: vec![],
                tasks: vec![],
                activities: vec![],
                recent_outcomes: vec![],
                unresolved_history: vec![],
                totals: ProgressTotals::default(),
            })
            .areas
            .push(area);
    }
    for card in cards.values().filter(|card| card.retained) {
        project_for(&mut projects, &card.card.area_id)
            .roles
            .push(card.clone());
    }
    for ((_, logical), tasks) in groups {
        let recent = recent_task_outcome(&logical, &tasks, &cards, recent_window);
        let summary = summarize(logical, tasks, &cards, recent_window);
        let project = project_for(&mut projects, &summary.card.area_id);
        if let Some(recent) = recent {
            project.recent_outcomes.push(recent);
        }
        project.tasks.push(summary);
    }
    for activity in activities.into_values() {
        let (summary, recent) = summarize_activity(activity, &cards, recent_window);
        let project = project_for(&mut projects, &summary.card.area_id);
        if let Some(recent) = recent {
            project.recent_outcomes.push(recent);
        }
        project.activities.push(summary);
    }
    for record in unresolved_history {
        project_for(&mut projects, &record.area_id)
            .unresolved_history
            .push(record);
    }
    let mut totals = ProgressTotals {
        project_count: projects
            .values()
            .filter(|p| p.project_identity_known)
            .count(),
        ..ProgressTotals::default()
    };
    for project in projects.values_mut() {
        project.recent_outcomes.sort_by_key(|r| {
            (
                std::cmp::Reverse(r.terminal_at_unix_ms),
                r.target.card.clone(),
                r.logical_task_ref.clone(),
                r.target.execution.clone(),
            )
        });
        project.totals.project_count = usize::from(project.project_identity_known);
        project.totals.area_count = project.areas.len();
        project.totals.role_count = project.roles.len();
        project.totals.unresolved_historical_record_count = project.unresolved_history.len();
        project.totals.all_known_logical_task_count = project.tasks.len();
        project.totals.unfinished_task_count = project
            .tasks
            .iter()
            .filter(|t| !t.phase.is_terminal())
            .count();
        project.totals.attention_task_count = project
            .tasks
            .iter()
            .filter(|t| t.requires_attention)
            .count();
        project.totals.recent_task_outcome_count = project
            .recent_outcomes
            .iter()
            .filter(|r| r.kind == ProgressOutcomeKind::LogicalTask)
            .count();
        project.totals.recent_activity_outcome_count = project
            .recent_outcomes
            .iter()
            .filter(|r| r.kind == ProgressOutcomeKind::DirectActivity)
            .count();
        project.totals.all_known_direct_activity_count = project.activities.len();
        project.totals.default_report_activity_count = project
            .activities
            .iter()
            .filter(|a| a.include_in_default_report)
            .count();
        project.totals.unfinished_activity_count = project
            .activities
            .iter()
            .filter(|a| !a.phase.is_terminal())
            .count();
        project.totals.attention_activity_count = project
            .activities
            .iter()
            .filter(|a| a.requires_attention)
            .count();
        for task in &project.tasks {
            *project
                .totals
                .all_known_task_phases
                .entry(task.phase)
                .or_default() += 1;
            if task.include_in_default_report {
                project.totals.default_report_task_count += 1;
                *project
                    .totals
                    .default_report_task_phases
                    .entry(task.phase)
                    .or_default() += 1;
            }
            for quality in &task.quality {
                *project
                    .totals
                    .quality_task_counts
                    .entry(*quality)
                    .or_default() += 1;
            }
        }
        totals.area_count += project.totals.area_count;
        totals.role_count += project.totals.role_count;
        totals.all_known_logical_task_count += project.totals.all_known_logical_task_count;
        totals.unfinished_task_count += project.totals.unfinished_task_count;
        totals.attention_task_count += project.totals.attention_task_count;
        totals.recent_task_outcome_count += project.totals.recent_task_outcome_count;
        totals.recent_activity_outcome_count += project.totals.recent_activity_outcome_count;
        totals.default_report_task_count += project.totals.default_report_task_count;
        for (phase, count) in &project.totals.default_report_task_phases {
            *totals.default_report_task_phases.entry(*phase).or_default() += count;
        }
        totals.all_known_direct_activity_count += project.totals.all_known_direct_activity_count;
        totals.default_report_activity_count += project.totals.default_report_activity_count;
        totals.unfinished_activity_count += project.totals.unfinished_activity_count;
        totals.attention_activity_count += project.totals.attention_activity_count;
        totals.unresolved_historical_record_count +=
            project.totals.unresolved_historical_record_count;
        for (phase, count) in &project.totals.all_known_task_phases {
            *totals.all_known_task_phases.entry(*phase).or_default() += count;
        }
        for (quality, count) in &project.totals.quality_task_counts {
            *totals.quality_task_counts.entry(*quality).or_default() += count;
        }
    }
    Ok(WorkspaceProgressSnapshot {
        scope: facts.scope,
        catalog_version: facts.catalog_version,
        queried_at_unix_ms: query.queried_at_unix_ms,
        timezone: query.timezone,
        recent_window,
        totals,
        requested_project_id: query.project_id,
        queried_area_ids: expected.into_iter().collect(),
        coverage,
        coverage_complete,
        projects: projects.into_values().collect(),
    })
}
fn project_for<'a>(
    projects: &'a mut BTreeMap<String, ProgressProjectSummary>,
    area: &str,
) -> &'a mut ProgressProjectSummary {
    projects
        .values_mut()
        .find(|p| p.areas.iter().any(|a| a.area_id == area))
        .expect("filtered area exists")
}

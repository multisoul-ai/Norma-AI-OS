use crate::app::OverviewQuestion;
use norma::mobile::{ApprovalSnapshot, RunOverview, RunOverviewAttentionKind};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Clone, Debug)]
pub(crate) struct IslandProject {
    pub source: u64,
    pub area_id: String,
    pub name: String,
    pub root: PathBuf,
    pub loaded: bool,
    pub overview: RunOverview,
    pub cached_overview: RunOverview,
    pub approvals: Vec<ApprovalSnapshot>,
    pub questions: Vec<OverviewQuestion>,
}

impl PartialEq for IslandProject {
    fn eq(&self, other: &Self) -> bool {
        // A refreshed fallback is not visible while a runtime is loaded.
        self.source == other.source
            && self.area_id == other.area_id
            && self.name == other.name
            && self.root == other.root
            && self.loaded == other.loaded
            && self.overview == other.overview
            && self.approvals == other.approvals
            && self.questions == other.questions
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IslandWorkspace {
    pub database: PathBuf,
    pub id: String,
    pub name: String,
    pub projects: Vec<IslandProject>,
}

struct Source {
    version: u64,
    database: PathBuf,
    workspaces: Vec<IslandWorkspace>,
}

struct Catalog {
    version: u64,
    workspaces: Vec<IslandWorkspace>,
}

/// Catalog membership/receipts have one latest version; each live runtime keeps its owner.
#[derive(Default)]
pub(crate) struct IslandModel {
    sequence: u64,
    sources: BTreeMap<u64, Source>,
    retired_sources: BTreeSet<u64>,
    catalogs: BTreeMap<PathBuf, Catalog>,
    pub workspaces: Vec<IslandWorkspace>,
}

impl IslandModel {
    pub fn begin_refresh(&mut self) -> u64 {
        self.sequence = self.sequence.saturating_add(1);
        self.sequence
    }

    /// Every publication contains the complete catalog for its database, including an empty one.
    pub fn replace_source(
        &mut self,
        source: u64,
        version: u64,
        database: PathBuf,
        workspaces: Vec<IslandWorkspace>,
    ) -> bool {
        if self.retired_sources.contains(&source)
            || self
                .sources
                .get(&source)
                .is_some_and(|old| old.version > version)
        {
            return false;
        }
        if self
            .catalogs
            .get(&database)
            .is_none_or(|old| old.version <= version)
        {
            self.catalogs.insert(
                database.clone(),
                Catalog {
                    version,
                    workspaces: workspaces.clone(),
                },
            );
        }
        self.sources.insert(
            source,
            Source {
                version,
                database,
                workspaces,
            },
        );
        self.rebuild()
    }

    pub fn remove_source(&mut self, source: u64) -> bool {
        self.retired_sources.insert(source);
        self.sources.remove(&source);
        self.rebuild()
    }

    fn rebuild(&mut self) -> bool {
        let mut merged = Vec::new();
        for (database, catalog) in &self.catalogs {
            // Preserve the watermark after release; expose only retained databases.
            if !self
                .sources
                .values()
                .any(|source| &source.database == database)
            {
                continue;
            }
            for workspace in &catalog.workspaces {
                let mut target = IslandWorkspace {
                    projects: Vec::new(),
                    ..workspace.clone()
                };
                for project in &workspace.projects {
                    let mut live: Vec<_> = self
                        .sources
                        .iter()
                        .filter(|(_, source)| &source.database == database)
                        .filter_map(|(id, source)| {
                            source
                                .workspaces
                                .iter()
                                .find(|ws| ws.id == workspace.id)
                                .and_then(|ws| {
                                    ws.projects
                                        .iter()
                                        .find(|p| p.area_id == project.area_id && p.loaded)
                                })
                                .map(|live| (*id, live.clone()))
                        })
                        .collect();
                    if live.is_empty() {
                        let mut cached = project.clone();
                        cached.source = self
                            .sources
                            .iter()
                            .find(|(_, source)| &source.database == database)
                            .map(|(id, _)| *id)
                            .unwrap_or_default();
                        cached.loaded = false;
                        cached.approvals.clear();
                        cached.questions.clear();
                        cached.overview = project.cached_overview.clone();
                        target.projects.push(cached);
                    } else {
                        for (index, (source, live)) in live.iter_mut().enumerate() {
                            live.source = *source;
                            live.name = project.name.clone();
                            live.root = project.root.clone();
                            live.overview
                                .attention
                                .retain(|row| !persisted_attention(row.kind));
                            if index == 0 {
                                live.overview.attention.extend(
                                    project
                                        .cached_overview
                                        .attention
                                        .iter()
                                        .filter(|row| persisted_attention(row.kind))
                                        .cloned(),
                                );
                            }
                            for row in &mut live.overview.attention {
                                row.project = project.name.clone();
                                row.loaded = true;
                            }
                            for row in live
                                .overview
                                .running
                                .iter_mut()
                                .chain(&mut live.overview.idle)
                            {
                                row.project = project.name.clone();
                            }
                        }
                        target
                            .projects
                            .extend(live.into_iter().map(|(_, project)| project));
                    }
                }
                merged.push(target);
            }
        }
        if merged == self.workspaces {
            return false;
        }
        self.workspaces = merged;
        true
    }
}

fn persisted_attention(kind: RunOverviewAttentionKind) -> bool {
    matches!(
        kind,
        RunOverviewAttentionKind::Review | RunOverviewAttentionKind::File
    )
}

#[cfg(test)]
#[path = "../../tests/unit/island_model_tests.rs"]
mod tests;

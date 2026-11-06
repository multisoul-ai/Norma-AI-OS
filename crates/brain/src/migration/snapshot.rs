use super::{proposal_file, records, workspace_hash, WorkspaceSource};
use crate::{persistence::atomic_write, preferences::AskRule};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

pub(super) struct Snapshot {
    goals: Vec<Value>,
    decisions: Vec<Value>,
    proposals: BTreeMap<String, Value>,
    settings: Value,
    pub archived_workspaces: Vec<String>,
}

impl Snapshot {
    pub fn write(&self, directory: &Path) -> io::Result<()> {
        atomic_write(
            &directory.join("goals.json"),
            &serde_json::to_vec(&self.goals)?,
        )?;
        atomic_write(
            &directory.join("config.json"),
            &serde_json::to_vec(&self.settings)?,
        )?;
        let mut decisions = Vec::new();
        for item in &self.decisions {
            serde_json::to_writer(&mut decisions, item)?;
            decisions.push(b'\n');
        }
        atomic_write(&directory.join("decisions.jsonl"), &decisions)?;
        let proposals = directory.join("proposals");
        fs::create_dir_all(&proposals)?;
        for (scope, value) in &self.proposals {
            atomic_write(
                &proposal_file(directory, scope),
                &serde_json::to_vec(value)?,
            )?;
        }
        File::open(proposals)?.sync_all()?;
        Ok(())
    }

    fn add_proposals(&mut self, path: &Path, scope: Option<&str>) -> io::Result<()> {
        if let Some(incoming) = records::proposals(path, scope)? {
            let id = incoming["workspace_id"]
                .as_str()
                .expect("validated scope")
                .to_owned();
            if let Some(target) = self.proposals.get_mut(&id) {
                records::merge_proposals(target, incoming, path)?;
            } else {
                self.proposals.insert(id, incoming);
            }
        }
        Ok(())
    }
}

fn entries(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = entries
        .map(|entry| entry.map(|e| e.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}

fn merge_settings(target: &mut Value, incoming: &Value) {
    if let (Some(target), Some(incoming)) = (target.as_object_mut(), incoming.as_object()) {
        for (key, value) in incoming {
            if key == "always_ask" {
                continue;
            }
            if let Some(existing) = target
                .get_mut(key)
                .filter(|v| v.is_object() && value.is_object())
            {
                merge_settings(existing, value);
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn add_settings(
    target: &mut Value,
    rules: &mut BTreeSet<AskRule>,
    path: &Path,
    scope: Option<&str>,
) -> io::Result<()> {
    let saved = records::settings(path)?;
    if saved.is_some() || scope.is_some() {
        let mut settings = saved.unwrap_or_else(|| json!({}));
        rules.extend(records::rules(&settings["always_ask"], scope, path)?);
        if scope.is_some() {
            // Project permission is not App-wide consent. Preserve the intersection,
            // including the old defaults for projects without a local config file.
            settings["enabled"] = json!(
                target["enabled"].as_bool().unwrap_or(true)
                    && settings["enabled"].as_bool().unwrap_or(false)
            );
            if settings.get("autonomy").is_none() {
                settings["autonomy"] = json!({});
            }
            for (key, default) in [
                ("readonly_commands", true),
                ("project_writes", true),
                ("assign_followups", false),
            ] {
                settings["autonomy"][key] = json!(
                    target["autonomy"][key].as_bool().unwrap_or(true)
                        && settings["autonomy"][key].as_bool().unwrap_or(default)
                );
            }
        }
        merge_settings(target, &settings);
    }
    Ok(())
}

pub(super) fn collect(base: &Path, sources: &[WorkspaceSource]) -> io::Result<Snapshot> {
    let mut data = Snapshot {
        goals: Vec::new(),
        decisions: Vec::new(),
        proposals: BTreeMap::new(),
        settings: json!({}),
        archived_workspaces: Vec::new(),
    };
    let mut rules = BTreeSet::new();
    let mut identities = BTreeMap::new();
    for source in sources {
        if source.workspace_id.trim().is_empty() {
            return Err(records::invalid(base, "empty workspace identity"));
        }
        identities.insert(
            workspace_hash(&source.workspace_id),
            source.workspace_id.clone(),
        );
    }
    for directory in entries(&base.join("projects"))? {
        if !directory.is_dir() {
            continue;
        }
        let hash = directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| records::invalid(&directory, "non-UTF8 project directory"))?;
        let scope = identities.get(hash).cloned().unwrap_or_else(|| {
            let archived = format!("archived:{hash}");
            data.archived_workspaces.push(archived.clone());
            archived
        });
        data.goals
            .extend(records::goals(&directory.join("goals.json"), Some(&scope))?);
        data.decisions.extend(records::decisions(
            &directory.join("decisions.jsonl"),
            Some(&scope),
        )?);
        data.add_proposals(&directory.join("proposals.json"), Some(&scope))?;
        add_settings(
            &mut data.settings,
            &mut rules,
            &directory.join("config.json"),
            Some(&scope),
        )?;
    }
    for path in entries(&base.join("proposals"))? {
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let hash = path
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| records::invalid(&path, "non-UTF8 legacy proposal filename"))?;
        let identities: BTreeSet<_> = sources
            .iter()
            .filter(|source| &workspace_hash(&source.root.to_string_lossy())[..16] == hash)
            .map(|source| source.workspace_id.clone())
            .collect();
        let scope = if identities.len() == 1 {
            identities.into_iter().next().expect("single identity")
        } else {
            // A root can belong to several canvases. Never duplicate a dispatchable proposal.
            let archived = format!("archived:root:{hash}");
            data.archived_workspaces.push(archived.clone());
            archived
        };
        data.add_proposals(&path, Some(&scope))?;
    }
    // Existing App settings override project defaults; an existing global snapshot is newest.
    add_settings(
        &mut data.settings,
        &mut rules,
        &base.join("config.json"),
        None,
    )?;
    let global = base.join("global");
    data.goals
        .extend(records::goals(&global.join("goals.json"), None)?);
    data.decisions
        .extend(records::decisions(&global.join("decisions.jsonl"), None)?);
    data.add_proposals(&global.join("proposals.json"), None)?;
    for path in entries(&global.join("proposals"))? {
        if path.extension().is_some_and(|ext| ext == "json") {
            data.add_proposals(&path, None)?;
        }
    }
    add_settings(
        &mut data.settings,
        &mut rules,
        &global.join("config.json"),
        None,
    )?;
    data.settings["always_ask"] = serde_json::to_value(rules)?;
    Ok(data)
}

/// Reuse must fail closed on damaged current data instead of importing old backups over it.
pub(super) fn validate(directory: &Path) -> io::Result<()> {
    if !fs::metadata(directory.join("proposals"))?.is_dir() {
        return Err(records::invalid(
            directory,
            "missing current proposal directory",
        ));
    }
    for name in ["goals.json", "config.json", "decisions.jsonl"] {
        if !fs::metadata(directory.join(name))?.is_file() {
            return Err(records::invalid(directory, "missing current data file"));
        }
    }
    records::goals(&directory.join("goals.json"), None)?;
    records::decisions(&directory.join("decisions.jsonl"), None)?;
    let settings_path = directory.join("config.json");
    let settings = records::settings(&settings_path)?.expect("current settings exist");
    records::rules(&settings["always_ask"], None, &settings_path)?;
    for path in entries(&directory.join("proposals"))? {
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let proposals = records::proposals(&path, None)?
            .ok_or_else(|| records::invalid(&path, "missing proposals"))?;
        let id = proposals["workspace_id"].as_str().expect("validated scope");
        if proposal_file(directory, id) != path {
            return Err(records::invalid(
                &path,
                "proposal filename differs from workspace identity",
            ));
        }
    }
    Ok(())
}

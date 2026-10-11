use super::*;

#[derive(Clone, Debug)]
pub struct WorkspaceRoleCandidate {
    pub card: WorkspaceCardRef,
    pub preferred_name: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRoleRename {
    pub card: WorkspaceCardRef,
    pub previous_name: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRoleReconciliation {
    pub roles: Vec<WorkspaceAgentRole>,
    pub renames: Vec<WorkspaceRoleRename>,
}

impl WorkspaceAgentRoleStore {
    /// Complete catalog scans call this once. Existing bindings and retired keys win.
    /// The immediate transaction arbitrates against concurrent create/rename operations.
    pub fn reconcile(
        &self,
        workspace_id: &str,
        candidates: &[WorkspaceRoleCandidate],
    ) -> StorageResult<WorkspaceRoleReconciliation> {
        let mut ordered = candidates.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|c| (&c.card.area_id, c.card.node_id, &c.card.card_instance_id));
        let mut seen = std::collections::HashSet::new();
        for c in &ordered {
            validate_card(&c.card)?;
            if c.card.workspace_id != workspace_id || !seen.insert(&c.card.card_instance_id) {
                return Err(StorageError::InvalidWorkspaceAgentRole(
                    "duplicate_scan_identity".into(),
                ));
            }
        }
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = current_unix_timestamp()?;
        let mut unresolved = Vec::new();
        let mut changed = false;
        // Reserve all legal legacy names first so an unnamed card cannot steal a later name.
        for c in &ordered {
            if let Some(existing) = read_role(&tx, &c.card)? {
                if existing.card != c.card {
                    return Err(StorageError::WorkspaceAgentCardConflict(
                        c.card.card_instance_id.clone(),
                    ));
                }
                continue;
            }
            let parsed = c
                .preferred_name
                .as_deref()
                .and_then(|n| AgentName::parse(n).ok());
            let usable = if let Some(name) = parsed {
                match reject_name_conflicts(&tx, workspace_id, &name, None) {
                    Ok(()) => Some(name),
                    Err(StorageError::AgentNameConflict { .. }) => None,
                    Err(error) => return Err(error),
                }
            } else {
                None
            };
            if let Some(name) = usable {
                insert_role(&tx, c, &name, now)?;
                changed = true;
            } else {
                unresolved.push(*c);
            }
        }
        let mut renames = Vec::new();
        for c in unresolved {
            let name = available_name(&tx, workspace_id)?;
            insert_role(&tx, c, &name, now)?;
            if let Some(old) = c
                .preferred_name
                .as_deref()
                .and_then(|n| AgentName::parse(n).ok())
            {
                for (key, display, kind) in name_keys(&old) {
                    tx.execute("INSERT OR IGNORE INTO workspace_agent_call_keys(workspace_id,normalized_key,card_instance_id,display,kind,state) VALUES(?1,?2,?3,?4,?5,'inactive')",params![workspace_id,key,c.card.card_instance_id,display,kind])?;
                }
            }
            tx.execute("INSERT INTO workspace_agent_role_repairs(workspace_id,card_instance_id,previous_name,name,repaired_at_unix) VALUES(?1,?2,?3,?4,?5)",params![workspace_id,c.card.card_instance_id,c.preferred_name,name.display(),now])?;
            renames.push(WorkspaceRoleRename {
                card: c.card.clone(),
                previous_name: c.preferred_name.clone(),
                name: name.display().into(),
            });
            changed = true;
        }
        if changed {
            bump_revision(&tx, workspace_id)?;
        }
        let roles = ordered
            .iter()
            .map(|c| {
                read_role(&tx, &c.card)?
                    .ok_or_else(|| StorageError::InvalidWorkspaceAgentRole("target_missing".into()))
            })
            .collect::<StorageResult<Vec<_>>>()?;
        tx.commit()?;
        Ok(WorkspaceRoleReconciliation { roles, renames })
    }

    /// Allocate a legal unoccupied name atomically; optional preferred name is a hint.
    pub fn create_auto(
        &self,
        card: &WorkspaceCardRef,
        preferred: Option<&str>,
        enabled: bool,
    ) -> StorageResult<WorkspaceAgentRole> {
        let report = self.reconcile(
            &card.workspace_id,
            &[WorkspaceRoleCandidate {
                card: card.clone(),
                preferred_name: preferred.map(str::to_owned),
                enabled,
            }],
        )?;
        report
            .roles
            .into_iter()
            .next()
            .ok_or_else(|| StorageError::InvalidWorkspaceAgentRole("target_missing".into()))
    }

    pub fn repairs(&self, workspace_id: &str) -> StorageResult<Vec<WorkspaceRoleRename>> {
        let Some(c) = self.read_connection()? else {
            return Ok(Vec::new());
        };
        let mut q = c.prepare("SELECT r.area_id,r.node_id,r.card_instance_id,p.previous_name,p.name FROM workspace_agent_role_repairs p JOIN workspace_agent_roles r USING(workspace_id,card_instance_id) WHERE p.workspace_id=?1 ORDER BY r.area_id,r.node_id,r.card_instance_id")?;
        let rows = q
            .query_map([workspace_id], |row| {
                Ok(WorkspaceRoleRename {
                    card: WorkspaceCardRef {
                        workspace_id: workspace_id.into(),
                        area_id: row.get(0)?,
                        node_id: row.get(1)?,
                        card_instance_id: row.get(2)?,
                    },
                    previous_name: row.get(3)?,
                    name: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

pub(super) fn insert_role(
    c: &Connection,
    candidate: &WorkspaceRoleCandidate,
    name: &AgentName,
    now: u64,
) -> StorageResult<()> {
    let card = &candidate.card;
    c.execute("INSERT INTO workspace_agent_roles(workspace_id,card_instance_id,area_id,node_id,name,speech_alias,enabled,created_at_unix,updated_at_unix) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",params![card.workspace_id,card.card_instance_id,card.area_id,card.node_id,name.display(),name.speech_alias(),candidate.enabled,now])?;
    insert_keys(c, card, name)
}

pub(super) fn available_name(c: &Connection, workspace: &str) -> StorageResult<AgentName> {
    let mut q = c.prepare("SELECT display FROM workspace_agent_call_keys WHERE workspace_id=?1")?;
    let occupied = q
        .query_map([workspace], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let registry = agent::AgentNameRegistry::from_names(occupied)
        .map_err(|_| StorageError::InvalidWorkspaceAgentRole("invalid_call_key".into()))?;
    parse_name(
        &registry
            .next_available_name()
            .ok_or_else(|| StorageError::InvalidWorkspaceAgentRole("name_pool_exhausted".into()))?,
    )
}

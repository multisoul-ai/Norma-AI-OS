use super::*;

impl WorkspaceAgentRoleStore {
    /// Production scope guard surrounds the two databases; a removed area never launches a card.
    pub fn materialize_workspace_card(
        &self,
        card: &WorkspaceCardRef,
        canvas: &crate::CanvasStore,
        node: &canvas::CanvasNodeSnapshot,
        explicit: bool,
    ) -> StorageResult<WorkspaceAgentRole> {
        let catalog = crate::DesktopWorkspaceStore::new(self.path.clone());
        let before = catalog
            .list_projects(&card.workspace_id)?
            .into_iter()
            .find(|a| a.id == card.area_id)
            .ok_or_else(|| StorageError::InvalidWorkspaceAgentRole("area_removed".into()))?;
        let result = self.materialize_card_with_policy(card, canvas, node, explicit);
        let current = catalog.list_projects(&card.workspace_id)?;
        if !current.iter().any(|a| {
            a.id == before.id && a.root == before.root && a.project_key == before.project_key
        }) {
            canvas.remove_card_instance(card.node_id, &card.card_instance_id)?;
            if self.get(card)?.is_some() {
                self.delete_after_card_removed(card)?;
            }
            return Err(StorageError::InvalidWorkspaceAgentRole(
                "area_removed".into(),
            ));
        }
        result
    }

    /// Background two-phase creation: durable unavailable intent, exact canvas append, then enable.
    /// Failures after a canvas write retain names for recovery; they never claim cross-DB atomicity.
    pub fn materialize_card(
        &self,
        card: &WorkspaceCardRef,
        canvas: &crate::CanvasStore,
        node: &canvas::CanvasNodeSnapshot,
    ) -> StorageResult<WorkspaceAgentRole> {
        self.materialize_card_with_policy(card, canvas, node, false)
    }

    /// Explicit user-entered names must fail on conflict instead of silently choosing someone else.
    pub fn materialize_named_card(
        &self,
        card: &WorkspaceCardRef,
        canvas: &crate::CanvasStore,
        node: &canvas::CanvasNodeSnapshot,
    ) -> StorageResult<WorkspaceAgentRole> {
        self.materialize_card_with_policy(card, canvas, node, true)
    }

    fn materialize_card_with_policy(
        &self,
        card: &WorkspaceCardRef,
        canvas: &crate::CanvasStore,
        node: &canvas::CanvasNodeSnapshot,
        explicit: bool,
    ) -> StorageResult<WorkspaceAgentRole> {
        validate_card(card)?;
        if node.id != card.node_id || node.card_instance_id != card.card_instance_id {
            return Err(StorageError::InvalidWorkspaceAgentRole(
                "creation_identity_mismatch".into(),
            ));
        }
        let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
            return Err(StorageError::InvalidWorkspaceAgentRole("not_agent".into()));
        };
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let name = match spec.alias.as_deref().and_then(|n| AgentName::parse(n).ok()) {
            Some(name) => match reject_name_conflicts(&tx, &card.workspace_id, &name, None) {
                Ok(()) => name,
                Err(StorageError::AgentNameConflict { .. }) if !explicit => {
                    reconciliation::available_name(&tx, &card.workspace_id)?
                }
                Err(error) => return Err(error),
            },
            None if explicit => {
                return Err(StorageError::InvalidAgentName(
                    spec.alias.clone().unwrap_or_default(),
                ))
            }
            None => reconciliation::available_name(&tx, &card.workspace_id)?,
        };
        reconciliation::insert_role(
            &tx,
            &WorkspaceRoleCandidate {
                card: card.clone(),
                preferred_name: Some(name.display().into()),
                enabled: false,
            },
            &name,
            current_unix_timestamp()?,
        )?;
        tx.execute("INSERT INTO workspace_agent_role_creation(workspace_id,card_instance_id,owner_pid) VALUES(?1,?2,?3)",params![card.workspace_id,card.card_instance_id,std::process::id()])?;
        bump_revision(&tx, &card.workspace_id)?;
        tx.commit()?;
        let mut node = node.clone();
        let canvas::CanvasNodeKind::Agent(spec) = &mut node.kind else {
            unreachable!()
        };
        spec.alias = Some(name.display().into());
        if let Err(error) = canvas.insert_reserved_card(&node) {
            // An acknowledged absent card permits cleanup. A read/IO uncertainty retains names.
            if matches!(
                canvas.contains_card_instance(card.node_id, &card.card_instance_id),
                Ok(false)
            ) {
                self.delete_after_card_removed(card)?;
            }
            return Err(error);
        }
        self.complete_creation(card)
    }

    pub(super) fn creation_pending(&self, card: &WorkspaceCardRef) -> StorageResult<bool> {
        let Some(c) = self.read_connection()? else {
            return Ok(false);
        };
        Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM workspace_agent_role_creation WHERE workspace_id=?1 AND card_instance_id=?2)",params![card.workspace_id,card.card_instance_id],|r|r.get(0))?)
    }

    fn complete_creation(&self, card: &WorkspaceCardRef) -> StorageResult<WorkspaceAgentRole> {
        let mut c = self.write_connection()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let role = read_role(&tx, card)?
            .ok_or_else(|| StorageError::InvalidWorkspaceAgentRole("target_missing".into()))?;
        tx.execute("DELETE FROM workspace_agent_role_creation WHERE workspace_id=?1 AND card_instance_id=?2",params![card.workspace_id,card.card_instance_id])?;
        tx.execute("UPDATE workspace_agent_roles SET enabled=1 WHERE workspace_id=?1 AND card_instance_id=?2",params![card.workspace_id,card.card_instance_id])?;
        bump_revision(&tx, &card.workspace_id)?;
        tx.commit()?;
        Ok(WorkspaceAgentRole {
            enabled: true,
            ..role
        })
    }

    pub(super) fn recover_creations(&self, workspace: &str) -> StorageResult<()> {
        let Some(c) = self.read_connection()? else {
            return Ok(());
        };
        let mut q = c.prepare(&format!("SELECT {},i.owner_pid FROM workspace_agent_roles r JOIN workspace_agent_role_creation i USING(workspace_id,card_instance_id) WHERE r.workspace_id=?1", ROLE_COLUMNS.split(',').map(|c| format!("r.{c}")).collect::<Vec<_>>().join(",")))?;
        let pending = q
            .query_map([workspace], |r| Ok((decode_role(r)?, r.get::<_, i32>(9)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        for (role, pid) in pending {
            // A live creator owns its intent. PID reuse conservatively retains the reservation.
            if pid > 0
                && (unsafe { libc::kill(pid, 0) } == 0
                    || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
            {
                continue;
            }
            let path = crate::project_store::desktop_canvas_path(
                &self.path,
                workspace,
                &role.card.area_id,
            )?;
            let present = path.try_exists()?
                && crate::CanvasStore::new(path)
                    .load_snapshot_with_card_identity()?
                    .is_some_and(|s| {
                        s.nodes.iter().any(|n| {
                            n.id == role.card.node_id
                                && n.card_instance_id == role.card.card_instance_id
                        })
                    });
            if present {
                self.complete_creation(&role.card)?;
            } else {
                self.delete_after_card_removed(&role.card)?;
            }
        }
        Ok(())
    }
}

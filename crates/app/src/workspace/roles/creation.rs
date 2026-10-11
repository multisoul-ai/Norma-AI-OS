use super::*;
use gpui::Window;

impl MultisoulApp {
    /// Takes only a card whose unique name and exact snapshot have committed in background.
    pub(crate) fn install_registered_agent_card(
        &mut self,
        node: canvas::CanvasNodeSnapshot,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let id = node.id;
        let bounds = node.bounds;
        if !self.runtime.document.install_reserved_node(node) {
            return;
        }
        self.mark_canvas_dirty();
        self.mark_session_dirty();
        if let Some(window) = window {
            crate::canvas::spawn_focus::activate_spawned_node(&mut self.runtime, id);
            self.motion.spawn_node(id, bounds);
            self.apply_layout_for_new_node(id, window);
        }
        let prepared = self.runtime.prepare_registered_agent_terminal(id, 80, 24);
        let Ok(launch) = prepared else {
            cx.notify();
            return;
        };
        let Ok(Some(launch)) = self
            .runtime
            .complete_registered_launch_with_factory(id, launch)
        else {
            cx.notify();
            return;
        };
        let run = launch.run_id().to_owned();
        let identity = self
            .runtime
            .document
            .node(id)
            .unwrap()
            .card_instance_id
            .clone();
        let work = cx
            .background_executor()
            .spawn(async move { launch.spawn() });
        cx.spawn(async move |this, cx| {
            let result = work.await.and_then(|p| p.into_live());
            let _ = this.update(cx, |app, cx| {
                if app
                    .runtime
                    .document
                    .node(id)
                    .is_some_and(|n| n.card_instance_id == identity)
                {
                    if let Err(error) = app.runtime.complete_native_agent_terminal(id, &run, result)
                    {
                        app.workspace_state.error = Some(
                            ui::i18n::LocalizedText::new("common.operation_failed")
                                .with_arg("reason", error.to_string()),
                        );
                    }
                    app.mark_session_dirty();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}

impl MultisoulApp {
    /// Shared async creation for desktop-area phone/configuration/typed adapters.
    pub(crate) fn begin_registered_card_creation(
        &mut self,
        spec: canvas::AgentSpec,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<u64, String>> {
        if let Some(id) = spec.definition_id {
            let valid = self.agent_definitions.definitions().iter().any(|d| {
                d.id == id
                    && d.enabled
                    && d.runtime_profile == spec.profile
                    && spec.alias.as_deref() == Some(d.name.as_str())
            });
            if !valid {
                return gpui::Task::ready(Err("definition_changed".into()));
            }
        }
        let runtime_instance = self.runtime.runtime_instance_id().to_owned();
        let scope = self
            .workspace_role_scope()
            .map(|(w, a)| (w.to_owned(), a.to_owned()));
        let project = self.persistence.project_store().cloned();
        let root = self
            .runtime
            .workspace_root()
            .map(std::path::Path::to_path_buf);
        let Some(((workspace, area), project, root)) =
            scope.zip(project).zip(root).map(|((s, p), r)| (s, p, r))
        else {
            return gpui::Task::ready(Err("workspace_missing".into()));
        };
        let bounds = self.next_tiled_node_bounds_for_area(
            self.mobile_canvas_area(window),
            self.runtime.document.viewport.zoom,
        );
        let reserved = self
            .runtime
            .document
            .reserve_node(canvas::CanvasNodeKind::Agent(spec), bounds);
        let card = WorkspaceCardRef {
            workspace_id: workspace,
            area_id: area,
            node_id: reserved.id,
            card_instance_id: reserved.card_instance_id.clone(),
        };
        let database = self.database_paths.app_db_path().to_owned();
        let publication = self.persistence.begin_card_publication();
        let work=cx.background_executor().spawn(async move {
            let canvas = project.canvas_store();
            let node_id = reserved.id;
            let identity = reserved.card_instance_id.clone();
            let result = (|| {
            let catalog=storage::DesktopWorkspaceStore::new(database.clone());
            let areas=catalog.list_projects(&card.workspace_id).map_err(|e|e.to_string())?;
            let area=areas.iter().find(|a|a.id==card.area_id && a.root==root).ok_or("configuration_changed")?;
            let identity=workspace::project::identify_project(&root).map_err(|_|"project_unavailable")?;
            if identity.root!=root || identity.key!=area.project_key {return Err("configuration_changed".into());}
            let roles=storage::WorkspaceAgentRoleStore::new(database);
            roles.register_workspace(&card.workspace_id).map_err(|e|e.to_string())?;
            let explicit = matches!(&reserved.kind,canvas::CanvasNodeKind::Agent(spec) if spec.definition_id.is_none() && spec.alias.is_some());
            let role=roles.materialize_workspace_card(&card,&canvas,&reserved,explicit).map_err(|e|e.to_string())?;
            let mut node=reserved;if let canvas::CanvasNodeKind::Agent(spec)=&mut node.kind {spec.alias=Some(role.name);}
            Ok::<_,String>((card,node))
            })();
            let absent = matches!(canvas.contains_card_instance(node_id, &identity), Ok(false));
            (result, absent)
        });
        cx.spawn(async move |this, cx| {
            let (result, absent) = work.await;
            let prepared = this
                .update(cx, |app, cx| {
                    let (card, node) = match result {
                        Ok(value) => value,
                        Err(error) => {
                            app.persistence
                                .finish_card_publication(&publication, false, absent);
                            return Err(error);
                        }
                    };
                    if app.runtime.runtime_instance_id() != runtime_instance
                        || app.workspace_role_scope()
                            != Some((card.workspace_id.as_str(), card.area_id.as_str()))
                    {
                        app.persistence
                            .finish_card_publication(&publication, false, absent);
                        return Err("outcome_unknown".into());
                    }
                    let id = node.id;
                    let installed = app.runtime.document.install_reserved_node(node);
                    app.persistence
                        .finish_card_publication(&publication, installed, absent);
                    if !installed {
                        return Err("card_changed".into());
                    }
                    app.mark_canvas_dirty();
                    app.mark_session_dirty();
                    cx.notify();
                    app.runtime
                        .prepare_registered_agent_terminal(id, 80, 24)
                        .and_then(|launch| {
                            app.runtime
                                .complete_registered_launch_with_factory(id, launch)
                        })
                        .map(|launch| (card, launch))
                        .map_err(|e| e.to_string())
                })
                .map_err(|e| e.to_string())??;
            let (card, launch) = prepared;
            let Some(launch) = launch else {
                return Ok(card.node_id);
            };
            let run = launch.run_id().to_owned();
            let prepared = cx
                .background_executor()
                .spawn(async move { launch.spawn() })
                .await;
            let live = prepared.and_then(|p| p.into_live());
            this.update(cx, |app, cx| {
                if app.workspace_card_ref(card.node_id).as_ref() != Some(&card) {
                    return Err("card_changed".into());
                }
                app.runtime
                    .complete_native_agent_terminal(card.node_id, &run, live)
                    .map_err(|e| e.to_string())?;
                app.mark_session_dirty();
                cx.notify();
                Ok(card.node_id)
            })
            .map_err(|e| e.to_string())?
        })
    }
}

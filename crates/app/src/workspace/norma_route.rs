use crate::app::MultisoulApp;
use gpui::{Context, Window};
use std::path::Path;

impl MultisoulApp {
    pub(crate) fn register_norma_workspace_route(
        &mut self,
        workspace_root: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let (Some(workspace_id), Some(route_id)) = (
            self.workspace_id.as_deref(),
            self.workspace_route_id.as_deref(),
        ) {
            crate::norma::endpoint::unregister_workspace(workspace_id, route_id, cx);
            self.agent_hook_server.unregister(cx);
            self.runtime.set_agent_hook_config(None);
        }

        let display_name = workspace_root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("Workspace");
        let identity = match self
            .workspace_identity_store
            .resolve_or_create(workspace_root, display_name)
        {
            Ok(identity) => identity,
            Err(error) => {
                tracing::warn!(%error, path = %workspace_root.display(), "failed to resolve Workspace identity");
                self.workspace_id = None;
                self.workspace_route_id = None;
                self.runtime.set_agent_hook_config(None);
                return;
            }
        };
        let route_id = match crate::norma::endpoint::new_workspace_route_id() {
            Ok(route_id) => route_id,
            Err(error) => {
                tracing::warn!(%error, "failed to generate Workspace route identity");
                self.workspace_id = None;
                self.workspace_route_id = None;
                self.runtime.set_agent_hook_config(None);
                return;
            }
        };

        let workspace_id = identity.workspace_id;
        if let Err(error) = crate::norma::endpoint::register_workspace(
            &workspace_id,
            &route_id,
            window.window_handle(),
            cx.weak_entity(),
            identity.display_name,
            identity.canonical_root,
            self.agent_definitions.definitions().len(),
            cx,
        ) {
            tracing::warn!(?error, %workspace_id, %route_id, "failed to register Workspace route");
            self.workspace_id = None;
            self.workspace_route_id = None;
            self.runtime.set_agent_hook_config(None);
            return;
        }

        self.workspace_id = Some(workspace_id.clone());
        self.workspace_route_id = Some(route_id.clone());
        self.agent_hook_server.rotate(cx);
        let runtime_config = self.agent_hook_server.runtime_config().map(|mut config| {
            config.workspace_id = workspace_id;
            config.workspace_route_id = route_id;
            config.norma_socket = norma::client::default_socket_path()
                .to_string_lossy()
                .into_owned();
            config
        });
        self.runtime.set_agent_hook_config(runtime_config);
    }
}

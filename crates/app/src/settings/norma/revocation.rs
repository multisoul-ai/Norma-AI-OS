use crate::app::MultisoulApp;
use gpui::{App, Context, Entity, EntityId};
use std::path::Path;

/// The persistence callback can outlive its settings view and its originating window.
pub(super) fn revoke_for_database(database: &Path, cx: &mut App) {
    crate::app::revoke_workspace_memory(database, cx);
    let mut visited = std::collections::HashSet::new();
    for session in registered(database, None, cx) {
        if visited.insert(session.entity_id())
            && session.read(cx).database_paths.app_db_path() == database
        {
            session.update(cx, |app, cx| app.stop_recalled_conversation(cx));
        }
    }
}

fn registered(database: &Path, borrowed: Option<EntityId>, cx: &App) -> Vec<Entity<MultisoulApp>> {
    let mut sessions = crate::workspace::sessions::for_database(database, cx);
    for host in crate::workspace::desktop::sessions::hosts(database, cx) {
        if Some(host.entity_id()) != borrowed {
            sessions.extend(host.read(cx).desktop_workspace.sessions.values().cloned());
            sessions.push(host);
        }
    }
    sessions
}

impl MultisoulApp {
    /// Recall preferences belong to a database, not to the settings window.
    pub(super) fn revoke_memory_context(&mut self, cx: &mut Context<Self>) {
        let database = self.database_paths.app_db_path().to_owned();
        crate::app::revoke_workspace_memory(&database, cx);
        let mut hosts = Vec::new();
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            hosts.push(host);
        }
        let mut sessions = registered(&database, Some(cx.entity_id()), cx);
        sessions.extend(self.desktop_workspace.sessions.values().cloned());
        // The sender is already borrowed. Collect first and never read it reentrantly.
        for host in hosts {
            if host.entity_id() != cx.entity_id()
                && host.read(cx).database_paths.app_db_path() == database
            {
                sessions.extend(host.read(cx).desktop_workspace.sessions.values().cloned());
                sessions.push(host);
            }
        }
        let mut visited = std::collections::HashSet::from([cx.entity_id()]);
        for session in sessions {
            if visited.insert(session.entity_id())
                && session.read(cx).database_paths.app_db_path() == database
            {
                session.update(cx, |app, cx| app.stop_recalled_conversation(cx));
            }
        }
        self.stop_recalled_conversation(cx);
    }

    fn stop_recalled_conversation(&mut self, cx: &mut Context<Self>) {
        self.agent_runtime.take();
        // Native contexts cannot retract facts already sent; restart only on user input.
        self.stop_gpt_live_voice(cx);
        self.stop_doubao_voice(cx);
        self.stop_continuous_listening(cx);
        self.cancel_agent_announcements();
    }
}

//! Native browser events translated into App state.
use super::*;

impl MultisoulApp {
    pub(crate) fn drain_browser_events(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        for event in self.browsers.drain_events() {
            let Some(event) = self.browsers.consume_event(event) else {
                continue;
            };
            match event {
                #[cfg(windows)]
                BrowserEvent::Native(_) => {
                    unreachable!("native envelopes must resolve at consumption")
                }
                #[cfg(windows)]
                BrowserEvent::NativeViewFailed { node_id, message } => {
                    self.browsers.fail_native_view(node_id);
                    self.workspace_state.error = Some(
                        ui::i18n::LocalizedText::new("cards.browser_start_failed")
                            .with_arg("message", message),
                    );
                    changed = true;
                }
                BrowserEvent::TitleChanged { node_id, title } => {
                    if self.set_browser_title(node_id, title) {
                        changed = true;
                    }
                }
                BrowserEvent::PreviewNavigation { node_id, url } => {
                    self.route_workbench_navigation(node_id, url, cx);
                    changed = true;
                }
                BrowserEvent::LocationChanged { node_id, url } => {
                    if self.update_desktop_browser_tab_location(node_id, url.clone(), cx)
                        || self.set_browser_url(node_id, url)
                    {
                        self.mark_canvas_dirty();
                        changed = true;
                    }
                }
                BrowserEvent::PageInteraction { node_id } => {
                    if self.runtime.document.node(node_id).is_some() {
                        self.runtime.selection.select_only(node_id);
                        self.runtime.focus.exit_to_canvas();
                        self.interaction.keyboard_selected_node = None;
                        self.mark_node_viewed_if_needed(node_id);
                        changed = true;
                    }
                }
                BrowserEvent::PageLoad { .. } => {}
                BrowserEvent::NavigateCurrentCard { node_id, url } => {
                    if self.update_desktop_browser_tab_location(node_id, url.clone(), cx)
                        || self.set_browser_url(node_id, url.clone())
                    {
                        self.browsers.load_url(node_id, &url);
                        self.mark_canvas_dirty();
                        changed = true;
                    }
                }
                BrowserEvent::ChromeAuthCookies {
                    node_id,
                    url,
                    batch,
                } => match self.browsers.apply_chrome_auth(node_id, &url, batch) {
                    Ok(Some((imported, skipped))) => {
                        tracing::info!(node_id, imported, skipped, "Chrome Browser auth imported");
                        changed = true;
                    }
                    Ok(None) => changed = true,
                    Err(message) => {
                        self.report_chrome_auth_failure(node_id, &message);
                        changed = true;
                    }
                },
                BrowserEvent::ChromeAuthImported {
                    node_id,
                    url,
                    imported,
                    skipped,
                } => {
                    if self.browsers.complete_chrome_auth(node_id, &url) {
                        tracing::info!(
                            node_id,
                            imported,
                            skipped,
                            "Chrome Browser auth imported asynchronously"
                        );
                        changed = true;
                    }
                }
                BrowserEvent::ChromeAuthFailed { node_id, message } => {
                    self.report_chrome_auth_failure(node_id, &message);
                    changed = true;
                }
            }
        }
        changed
    }

    fn report_chrome_auth_failure(&mut self, node_id: NodeId, message: &str) {
        self.browsers.fail_chrome_auth(node_id, message);
        self.workspace_state.error = Some(
            ui::i18n::LocalizedText::new("cards.chrome_sync_failed").with_arg("message", message),
        );
    }
}

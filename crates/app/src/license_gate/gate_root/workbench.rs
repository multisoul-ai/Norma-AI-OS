use super::*;

impl GateRoot {
    pub(super) fn mount_workbench(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            self.license_video_wallpaper = None;
        }
        let (database_paths, app_store) = match crate::database::default_database_context() {
            Ok(context) => context,
            Err(error) => {
                self.status = ShellStatus::Error(ui::i18n::message(
                    "license.could_not_initialize_the_database_error",
                    &[("error", (error).to_string())],
                ));
                self.enter_workbench = false;
                cx.notify();
                return;
            }
        };
        self.mount_workbench_from_context(database_paths, app_store, window, cx);
    }

    fn mount_workbench_from_context(
        &mut self,
        database_paths: storage::DatabasePaths,
        app_store: storage::AppStore,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let database = database_paths.app_db_path().to_owned();
        let workbench =
            if let Some(retained) = crate::workspace::desktop::sessions::detached(&database, cx) {
                retained.update(cx, |app, cx| app.attach_desktop_host_window(window, cx));
                retained
            } else {
                cx.new(|cx| {
                    let mut app =
                        MultisoulApp::new_with_database_context(cx, database_paths, app_store);
                    #[cfg(test)]
                    app.browsers.disable_native_views();
                    app.start_dialogue_turn_loop(window, cx);
                    app.start_cef_message_pump_loop(window, cx);
                    app.warm_up_browser_engine(window);
                    app.configure_cpu_benchmark_from_environment(window, cx);
                    #[cfg(feature = "e2e-test-support")]
                    if crate::workspace::windowing::visual_preview_name().as_deref()
                        == Some("github-clone")
                    {
                        app.open_clone_project_dialog(window, cx);
                    }
                    app
                })
            };
        crate::workspace::desktop::sessions::register(
            database,
            window.window_handle(),
            workbench.clone(),
            cx,
        );
        self.workbench = Some(workbench);
        self.phase = BootPhase::Workbench;
        self.enter_workbench = false;
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/gated_desktop_host_tests.rs"]
mod tests;

use super::state::CloneProjectPhase;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, relative, rgba, AnyElement, Context, Div, FontWeight, IntoElement, Window,
};
use ui::component::button::{Button, ButtonVariants};
use ui::component::input::Input;
use ui::component::Disableable;
use workspace::git_clone::CloneTransport;

impl MultisoulApp {
    pub(crate) fn render_clone_project_dialog(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let Some(dialog) = self.clone_project.dialog() else {
            return div().into_any_element();
        };
        let remote_input = dialog.remote_input.clone();
        let folder_input = dialog.folder_input.clone();
        let transport = dialog.transport;
        let parent = dialog.parent.display().to_string();
        let target_preview = self
            .clone_project
            .preview_target()
            .map(|path| path.display().to_string());
        let busy = self.clone_project.is_busy();
        let phase = dialog.phase.clone();
        let validation = self.clone_project.prepare();
        let validation_error = if matches!(phase, CloneProjectPhase::Editing)
            && !dialog.remote_draft.trim().is_empty()
        {
            validation.as_ref().err().cloned()
        } else {
            None
        };
        let can_clone = !busy && validation.is_ok();

        div()
            .id("clone-project-overlay")
            .debug_selector(|| "clone-project-overlay".to_string())
            .absolute()
            .top(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .left(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .bg(rgba(0x00000082))
            .child(
                div()
                    .id("clone-project-dialog")
                    .debug_selector(|| "clone-project-dialog".to_string())
                    .w(px(560.0))
                    .max_w(relative(0.92))
                    .flex()
                    .flex_col()
                    .rounded(px(18.0))
                    .border_1()
                    .border_color(rgba(ui::icons::with_alpha(
                        theme.node_stroke,
                        0xb8,
                    )))
                    .bg(rgba(theme.panel_high))
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .px(px(24.0))
                            .pt(px(22.0))
                            .pb(px(18.0))
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(theme.text_primary))
                                    .child("Clone GitHub Repository"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(19.0))
                                    .text_color(rgba(theme.text_secondary))
                                    .child(
                                        "Paste an HTTPS or SSH repository URL. Norma will clone it without running project setup scripts.",
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .px(px(24.0))
                            .pb(px(20.0))
                            .child(field_label(theme, "Repository URL"))
                            .child(input_surface(
                                theme,
                                "clone-project-remote-input",
                                Input::new(&remote_input)
                                    .appearance(false)
                                    .disabled(busy)
                                    .h(px(38.0)),
                            ))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        Button::new("clone-transport-https")
                                            .debug_selector(|| {
                                                "clone-transport-https".to_string()
                                            })
                                            .label("HTTPS")
                                            .compact()
                                            .disabled(busy)
                                            .when(transport == CloneTransport::Https, |button| {
                                                button.primary()
                                            })
                                            .on_click(cx.listener(|app, _, _, cx| {
                                                if let Some(dialog) = app.clone_project.dialog_mut()
                                                {
                                                    dialog.transport = CloneTransport::Https;
                                                    dialog.phase = CloneProjectPhase::Editing;
                                                }
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Button::new("clone-transport-ssh")
                                            .debug_selector(|| "clone-transport-ssh".to_string())
                                            .label("SSH")
                                            .compact()
                                            .disabled(busy)
                                            .when(transport == CloneTransport::Ssh, |button| {
                                                button.primary()
                                            })
                                            .on_click(cx.listener(|app, _, _, cx| {
                                                if let Some(dialog) = app.clone_project.dialog_mut()
                                                {
                                                    dialog.transport = CloneTransport::Ssh;
                                                    dialog.phase = CloneProjectPhase::Editing;
                                                }
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(field_label(theme, "Local folder name (optional)"))
                            .child(input_surface(
                                theme,
                                "clone-project-folder-input",
                                Input::new(&folder_input)
                                    .appearance(false)
                                    .disabled(busy)
                                    .h(px(38.0)),
                            ))
                            .child(field_label(theme, "Clone into"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .child(
                                        div()
                                            .id("clone-project-parent-path")
                                            .debug_selector(|| {
                                                "clone-project-parent-path".to_string()
                                            })
                                            .h(px(40.0))
                                            .flex_1()
                                            .flex()
                                            .items_center()
                                            .px(px(12.0))
                                            .rounded(px(9.0))
                                            .border_1()
                                            .border_color(rgba(
                                                ui::icons::with_alpha(
                                                    theme.node_stroke,
                                                    0x88,
                                                ),
                                            ))
                                            .text_size(px(12.0))
                                            .text_color(rgba(theme.text_secondary))
                                            .truncate()
                                            .child(parent),
                                    )
                                    .child(
                                        Button::new("clone-project-choose-parent")
                                            .debug_selector(|| {
                                                "clone-project-choose-parent".to_string()
                                            })
                                            .label("Choose…")
                                            .disabled(busy)
                                            .on_click(cx.listener(|app, _, window, cx| {
                                                app.choose_clone_parent(window, cx);
                                            })),
                                    ),
                            )
                            .when_some(target_preview, |container, target| {
                                container.child(
                                    div()
                                        .id("clone-project-target-preview")
                                        .debug_selector(|| {
                                            "clone-project-target-preview".to_string()
                                        })
                                        .text_size(px(12.0))
                                        .text_color(rgba(theme.text_secondary))
                                        .child(format!("Project will be created at {target}")),
                                )
                            })
                            .when_some(validation_error, |container, error| {
                                container.child(status_message(theme, error, false))
                            })
                            .when_some(phase_status(&phase), |container, status| {
                                container.child(status_message(
                                    theme,
                                    status.message,
                                    status.warning,
                                ))
                            })
                            .when_some(progress_for_phase(&phase), |container, progress| {
                                container.child(progress_view(theme, progress))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(10.0))
                            .px(px(24.0))
                            .py(px(16.0))
                            .border_t_1()
                            .border_color(rgba(ui::icons::with_alpha(
                                theme.node_stroke,
                                0x68,
                            )))
                            .child(
                                Button::new("clone-project-cancel")
                                    .debug_selector(|| "clone-project-cancel".to_string())
                                    .label(if busy { "Cancel Clone" } else { "Cancel" })
                                    .on_click(cx.listener(|app, _, _, cx| {
                                        app.cancel_or_close_clone_project(cx);
                                    })),
                            )
                            .child(
                                Button::new("clone-project-submit")
                                    .debug_selector(|| "clone-project-submit".to_string())
                                    .label(if busy { "Cloning…" } else { "Clone" })
                                    .primary()
                                    .disabled(!can_clone)
                                    .on_click(cx.listener(|app, _, window, cx| {
                                        app.start_clone_project(window, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}

struct PhaseStatus {
    message: String,
    warning: bool,
}

fn phase_status(phase: &CloneProjectPhase) -> Option<PhaseStatus> {
    match phase {
        CloneProjectPhase::Editing | CloneProjectPhase::Running(_) => None,
        CloneProjectPhase::Cancelling => Some(PhaseStatus {
            message: "Stopping Git and cleaning the incomplete clone…".to_string(),
            warning: false,
        }),
        CloneProjectPhase::Failed {
            message,
            target_preserved,
        } => Some(PhaseStatus {
            message: message.clone(),
            warning: *target_preserved,
        }),
    }
}

fn progress_for_phase(phase: &CloneProjectPhase) -> Option<workspace::git_clone::CloneProgress> {
    match phase {
        CloneProjectPhase::Running(progress) => Some(*progress),
        CloneProjectPhase::Editing
        | CloneProjectPhase::Cancelling
        | CloneProjectPhase::Failed { .. } => None,
    }
}

fn field_label(theme: ui::Theme, label: &'static str) -> Div {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(theme.text_secondary))
        .child(label)
}

fn input_surface(theme: ui::Theme, selector: &'static str, input: Input) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .h(px(40.0))
        .rounded(px(9.0))
        .border_1()
        .border_color(rgba(ui::icons::with_alpha(theme.node_stroke, 0x88)))
        .bg(rgba(ui::icons::with_alpha(theme.canvas_from, 0x88)))
        .child(input)
}

fn status_message(theme: ui::Theme, message: String, warning: bool) -> impl IntoElement {
    div()
        .id("clone-project-status")
        .debug_selector(|| "clone-project-status".to_string())
        .rounded(px(9.0))
        .px(px(12.0))
        .py(px(9.0))
        .bg(rgba(if warning { 0x8b5a002b } else { 0xa430302b }))
        .text_size(px(12.0))
        .line_height(px(18.0))
        .text_color(rgba(if warning {
            theme.accent_orange
        } else {
            0xffa4a4ff
        }))
        .child(message)
}

fn progress_view(
    theme: ui::Theme,
    progress: workspace::git_clone::CloneProgress,
) -> impl IntoElement {
    let fill = progress.percent.unwrap_or(0);
    div()
        .id("clone-project-progress")
        .debug_selector(|| "clone-project-progress".to_string())
        .flex()
        .flex_col()
        .gap(px(7.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_secondary))
                .child(progress.stage.label())
                .when_some(progress.percent, |row, percent| {
                    row.child(format!("{percent}%"))
                }),
        )
        .child(
            div()
                .h(px(5.0))
                .w_full()
                .rounded(px(3.0))
                .bg(rgba(ui::icons::with_alpha(theme.node_stroke, 0x54)))
                .child(
                    div()
                        .h_full()
                        .w(relative(f32::from(fill) / 100.0))
                        .rounded(px(3.0))
                        .bg(rgba(theme.accent_blue)),
                ),
        )
}

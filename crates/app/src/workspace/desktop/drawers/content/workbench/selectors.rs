use super::*;
use crate::app::MultisoulApp;
use gpui::{AppContext, Context, Window};
impl MultisoulApp {
    pub(super) fn ensure_workbench_selectors(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use ui::component::select::{SearchableVec, SelectEvent, SelectState};
        let choices = self
            .desktop_workspace
            .areas
            .iter()
            .map(|a| (a.id.clone(), a.display_name.clone()))
            .collect::<Vec<_>>();
        let s = &mut self.desktop_workspace.workbench;
        if s.project_picker.is_none() || s.project_choices != choices {
            s.project_choices = choices.clone();
            let items = choices
                .into_iter()
                .map(|(id, title)| Choice { id, title })
                .collect::<Vec<_>>();
            let picker = cx.new(|cx| {
                SelectState::new(SearchableVec::new(items), None, window, cx).searchable(true)
            });
            cx.subscribe(
                &picker,
                |app, _, event: &SelectEvent<SearchableVec<Choice>>, cx| {
                    if let SelectEvent::Confirm(Some(id)) = event {
                        let s = &mut app.desktop_workspace.workbench;
                        if s.busy {
                            return;
                        }
                        if s.saved.area_id.as_ref() != Some(id) {
                            s.saved.area_id = Some(id.clone());
                            s.saved.selected = None;
                            s.saved.expanded.clear();
                            s.file_generation = s.file_generation.wrapping_add(1);
                            s.file = None;
                            s.preview = None;
                        }
                        app.refresh_file_workbench(cx);
                    }
                },
            )
            .detach();
            s.project_picker = Some(picker);
        }
        if let (Some(picker), Some(id)) = (&s.project_picker, &s.saved.area_id) {
            if picker.read(cx).selected_value() != Some(id) {
                picker.update(cx, |p, cx| p.set_selected_value(id, window, cx));
            }
        }
        let branches = s
            .git
            .as_ref()
            .map(|g| g.branches.clone())
            .unwrap_or_default();
        if s.branch_picker.is_none() || s.branch_choices != branches {
            s.branch_choices = branches.clone();
            let items = branches
                .into_iter()
                .map(|id| Choice {
                    title: id.clone(),
                    id,
                })
                .collect::<Vec<_>>();
            let picker = cx.new(|cx| {
                SelectState::new(SearchableVec::new(items), None, window, cx).searchable(true)
            });
            cx.subscribe(
                &picker,
                |app, _, event: &SelectEvent<SearchableVec<Choice>>, cx| {
                    if let SelectEvent::Confirm(Some(id)) = event {
                        app.desktop_workspace.workbench.saved.comparison =
                            Comparison::Branch(id.clone());
                        app.refresh_file_workbench(cx);
                    }
                },
            )
            .detach();
            s.branch_picker = Some(picker);
        }
        if let (Some(picker), Comparison::Branch(id)) = (&s.branch_picker, &s.saved.comparison) {
            if picker.read(cx).selected_value() != Some(id) {
                picker.update(cx, |p, cx| p.set_selected_value(id, window, cx));
            }
        }
    }
}

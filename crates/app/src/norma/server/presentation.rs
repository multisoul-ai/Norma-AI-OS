use super::{NormaCommandEffects, NormaExecution};
use crate::app::MultisoulApp;
use crate::canvas::master_layout::{
    enter_master_layout_for_area, layout_gap_for_zoom, switch_master_layout_for_area,
};
use crate::motion::MotionStore;
use canvas::{CanvasNodeKind, CanvasRect, PreviewType as CanvasPreviewType};
use norma::presentation::validate_prepared_target;
use norma::protocol::{
    CardCommandError, CardCommandResult, PresentationDisposition, PresentationErrorCode,
    PresentationKind, PresentationLayout, PresentationTarget, PreviewType,
};
use runtime::Runtime;
use std::path::Path;

pub(super) fn execute_presentation_command(
    app: &mut MultisoulApp,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<MultisoulApp>,
    target: PresentationTarget,
    create_new: bool,
    source: agent::AgentDispatchSource,
) -> Result<NormaExecution, CardCommandError> {
    let source_name = source
        .definition_id
        .and_then(|definition_id| {
            app.agent_definitions
                .definitions()
                .iter()
                .find(|definition| definition.id == definition_id)
                .map(|definition| definition.name.clone())
        })
        .unwrap_or_else(|| "Agent".to_string());
    let area = app.visible_canvas_rect(window);
    let zoom = app.runtime.document.viewport.zoom;
    let execution = execute_presentation_command_for_area(
        &mut app.runtime,
        &mut app.motion,
        &mut app.master_layout,
        target,
        create_new,
        area,
        zoom,
    )?;
    if let CardCommandResult::Presented {
        node_id,
        kind: PresentationKind::Preview,
        ..
    } = &execution.result
    {
        app.refresh_preview_node(*node_id, cx);
    }
    app.release_all_browser_input();
    app.focus_handle.focus(window);
    app.show_presentation_notification(source_name, cx);
    Ok(execution)
}

pub(super) fn execute_presentation_command_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    target: PresentationTarget,
    create_new: bool,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    validate_prepared_target(&target).map_err(|error| error.into_card_error())?;
    let existing = (!create_new)
        .then(|| find_matching_node(runtime, &target))
        .flatten();
    let (node_id, disposition) = match existing {
        Some(node_id) => {
            update_reused_preview_type(runtime, node_id, &target);
            (node_id, PresentationDisposition::Reused)
        }
        None => (
            create_presentation_node(runtime, motion, &target, area, zoom),
            PresentationDisposition::Created,
        ),
    };

    let arranged = if master_layout.is_active() {
        switch_master_layout_for_area(master_layout, runtime, motion, node_id, area, zoom)
    } else {
        enter_master_layout_for_area(master_layout, runtime, motion, node_id, area, zoom)
    };
    if !arranged {
        if disposition == PresentationDisposition::Created {
            runtime.remove_node(node_id);
            motion.retain_nodes(runtime.document.nodes().iter().map(|node| node.id));
        }
        return Err(CardCommandError::presentation(
            PresentationErrorCode::PresentationFailed,
            "Could not arrange the requested card as Master",
            false,
        ));
    }

    runtime.focus.exit_to_canvas();
    Ok(NormaExecution {
        result: CardCommandResult::Presented {
            node_id,
            kind: presentation_kind(&target),
            disposition,
            layout: PresentationLayout::Master,
        },
        effects: NormaCommandEffects::CANVAS,
        removed_agent_definition_id: None,
    })
}

fn find_matching_node(runtime: &Runtime, target: &PresentationTarget) -> Option<u64> {
    runtime.document.nodes().iter().find_map(|node| {
        let matches = match (&node.kind, target) {
            (CanvasNodeKind::Browser(spec), PresentationTarget::Browser { url }) => {
                browser_urls_match(&spec.url, url)
            }
            (CanvasNodeKind::Preview(spec), PresentationTarget::Preview { path, .. }) => {
                Path::new(&spec.path) == path
            }
            _ => false,
        };
        matches.then_some(node.id)
    })
}

fn browser_urls_match(current: &str, requested: &str) -> bool {
    canonical_browser_url(current)
        .zip(canonical_browser_url(requested))
        .is_some_and(|(current, requested)| current == requested)
}

fn canonical_browser_url(value: &str) -> Option<String> {
    let normalized = norma::presentation::normalize_browser_url(value).ok()?;
    url::Url::parse(&normalized).ok().map(|url| url.to_string())
}

fn create_presentation_node(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    target: &PresentationTarget,
    area: CanvasRect,
    zoom: f32,
) -> u64 {
    let bounds = area.inset(layout_gap_for_zoom(zoom));
    let node_id = match target {
        PresentationTarget::Browser { url } => runtime.spawn_browser(url.clone(), None, bounds),
        PresentationTarget::Preview {
            path,
            explicit_type,
        } => runtime.spawn_preview_from_path(
            path.clone(),
            explicit_type.map(canvas_preview_type),
            bounds,
        ),
    };
    motion.spawn_node(node_id, bounds);
    node_id
}

fn update_reused_preview_type(runtime: &mut Runtime, node_id: u64, target: &PresentationTarget) {
    let PresentationTarget::Preview { explicit_type, .. } = target else {
        return;
    };
    let Some(node) = runtime.document.node_mut(node_id) else {
        return;
    };
    let CanvasNodeKind::Preview(spec) = &mut node.kind else {
        return;
    };
    spec.explicit_type = explicit_type.map(canvas_preview_type);
}

fn presentation_kind(target: &PresentationTarget) -> PresentationKind {
    match target {
        PresentationTarget::Browser { .. } => PresentationKind::Browser,
        PresentationTarget::Preview { .. } => PresentationKind::Preview,
    }
}

fn canvas_preview_type(preview_type: PreviewType) -> CanvasPreviewType {
    match preview_type {
        PreviewType::Markdown => CanvasPreviewType::Markdown,
        PreviewType::Text => CanvasPreviewType::Text,
        PreviewType::Image => CanvasPreviewType::Image,
    }
}

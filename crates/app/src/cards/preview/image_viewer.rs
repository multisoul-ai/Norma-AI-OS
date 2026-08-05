use super::card::{preview_surface_background, with_alpha};
use crate::preview::{ImageViewerPresentation, PreviewImageCommand, PreviewImageCommandKind};
use canvas::NodeId;
use gpui::{
    div, img, prelude::*, px, relative, rgba, AnyElement, CursorStyle, MouseButton, ObjectFit,
};
use std::path::PathBuf;
use ui::component::button::{Button, ButtonVariants};
use ui::component::{Disableable, IconName, Sizable};
use ui::Theme;

const IMAGE_PADDING_PX: f32 = 12.0;
const IMAGE_TOOLBAR_H_PX: f32 = 36.0;

pub(super) fn render_image_viewer(
    node_id: NodeId,
    paths: &[PathBuf],
    presentation: ImageViewerPresentation,
    dragging: bool,
    theme: Theme,
    canvas_zoom: f32,
    bottom_radius_px: f32,
) -> AnyElement {
    let Some(path) = paths.get(presentation.index).cloned() else {
        return viewer_message(
            "No image available",
            theme.destructive,
            theme,
            bottom_radius_px,
        );
    };
    let fallback_theme = theme;
    let loading_theme = theme;
    let pan_enabled = presentation.zoom > 1.0;
    let cursor = if dragging {
        CursorStyle::ClosedHand
    } else if presentation.zoom > 1.0 {
        CursorStyle::OpenHand
    } else {
        CursorStyle::default()
    };
    let viewport = div()
        .id(("preview-image-viewport", node_id))
        .debug_selector(|| "preview-image-viewport".to_string())
        .size_full()
        .overflow_scroll()
        .track_scroll(&presentation.scroll_handle)
        .cursor(cursor)
        .when(pan_enabled, move |viewport| {
            viewport.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(
                    Box::new(PreviewImageCommand {
                        node_id,
                        command: PreviewImageCommandKind::BeginPan {
                            screen_x: f32::from(event.position.x),
                            screen_y: f32::from(event.position.y),
                        },
                    }),
                    cx,
                );
            })
        })
        .child(
            div()
                .w(relative(presentation.zoom))
                .h(relative(presentation.zoom))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    img(path)
                        .size_full()
                        .object_fit(ObjectFit::Contain)
                        .with_loading(move || {
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgba(loading_theme.text_tertiary))
                                .child("Loading image...")
                                .into_any_element()
                        })
                        .with_fallback(move || {
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgba(fallback_theme.destructive))
                                .child("Could not decode image")
                                .into_any_element()
                        }),
                ),
        );

    div()
        .id(("preview-image-viewer", node_id))
        .debug_selector(|| "preview-image-viewer".to_string())
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .rounded_b(px(bottom_radius_px))
        .bg(rgba(preview_surface_background(theme)))
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .p(px(IMAGE_PADDING_PX * canvas_zoom))
                .child(viewport),
        )
        .child(render_toolbar(
            node_id,
            &presentation,
            theme,
            canvas_zoom,
            bottom_radius_px,
        ))
        .into_any_element()
}

fn render_toolbar(
    node_id: NodeId,
    presentation: &ImageViewerPresentation,
    theme: Theme,
    canvas_zoom: f32,
    bottom_radius_px: f32,
) -> impl IntoElement {
    let button_size = px((24.0 * canvas_zoom).clamp(22.0, 30.0));
    let navigation = div().flex().items_center().gap(px(3.0 * canvas_zoom)).when(
        presentation.image_count > 1,
        |navigation| {
            navigation
                .child(image_icon_button(
                    "preview-image-previous",
                    node_id,
                    IconName::ChevronLeft,
                    "Previous image",
                    PreviewImageCommandKind::Previous,
                    !presentation.can_go_previous,
                    button_size,
                ))
                .child(
                    div()
                        .debug_selector(|| "preview-image-counter".to_string())
                        .min_w(px(42.0 * canvas_zoom))
                        .text_center()
                        .text_size(px((11.0 * canvas_zoom).max(9.0)))
                        .text_color(rgba(theme.text_secondary))
                        .child(format!(
                            "{} / {}",
                            presentation.index.saturating_add(1),
                            presentation.image_count
                        )),
                )
                .child(image_icon_button(
                    "preview-image-next",
                    node_id,
                    IconName::ChevronRight,
                    "Next image",
                    PreviewImageCommandKind::Next,
                    !presentation.can_go_next,
                    button_size,
                ))
        },
    );
    let zoom_percent = format!("{:.0}%", presentation.zoom * 100.0);
    let zoom_controls = div()
        .flex()
        .items_center()
        .gap(px(3.0 * canvas_zoom))
        .child(image_icon_button(
            "preview-image-zoom-out",
            node_id,
            IconName::Minus,
            "Zoom out",
            PreviewImageCommandKind::ZoomOut,
            !presentation.can_zoom_out,
            button_size,
        ))
        .child(image_text_button(
            "preview-image-fit",
            node_id,
            zoom_percent,
            "Fit image",
            PreviewImageCommandKind::Fit,
            button_size,
        ))
        .child(image_icon_button(
            "preview-image-zoom-in",
            node_id,
            IconName::Plus,
            "Zoom in",
            PreviewImageCommandKind::ZoomIn,
            presentation.zoom >= 8.0,
            button_size,
        ));

    div()
        .debug_selector(|| "preview-image-toolbar".to_string())
        .h(px(IMAGE_TOOLBAR_H_PX * canvas_zoom))
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .px(px(8.0 * canvas_zoom))
        .rounded_b(px(bottom_radius_px))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x58)))
        .bg(rgba(with_alpha(theme.panel_high, 0xfa)))
        .child(navigation)
        .child(zoom_controls)
}

fn image_icon_button(
    id: &'static str,
    node_id: NodeId,
    icon: IconName,
    tooltip: &'static str,
    command: PreviewImageCommandKind,
    disabled: bool,
    size: gpui::Pixels,
) -> impl IntoElement {
    let button = Button::new((id, node_id))
        .debug_selector(move || id.to_string())
        .icon(icon)
        .tooltip(tooltip)
        .ghost()
        .compact()
        .with_size(size)
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            window.dispatch_action(Box::new(PreviewImageCommand { node_id, command }), cx);
        });
    div()
        .flex_none()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(button)
}

fn image_text_button(
    id: &'static str,
    node_id: NodeId,
    label: String,
    tooltip: &'static str,
    command: PreviewImageCommandKind,
    size: gpui::Pixels,
) -> impl IntoElement {
    let button = Button::new((id, node_id))
        .debug_selector(move || id.to_string())
        .label(label)
        .tooltip(tooltip)
        .ghost()
        .compact()
        .h(size)
        .on_click(move |_, window, cx| {
            window.dispatch_action(Box::new(PreviewImageCommand { node_id, command }), cx);
        });
    div()
        .flex_none()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(button)
}

fn viewer_message(
    message: &'static str,
    color: u32,
    theme: Theme,
    bottom_radius_px: f32,
) -> AnyElement {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .rounded_b(px(bottom_radius_px))
        .bg(rgba(preview_surface_background(theme)))
        .text_color(rgba(color))
        .child(message)
        .into_any_element()
}

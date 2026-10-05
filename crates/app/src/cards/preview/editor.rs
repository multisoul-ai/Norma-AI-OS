use super::card::preview_surface_background;
use crate::preview::{PreviewEditCommand, PreviewEditCommandKind, PreviewEditor};
use canvas::NodeId;
use gpui::{div, prelude::*, px, rgba, AnyElement, MouseButton};
use ui::{
    component::{
        button::{Button, ButtonVariants},
        input::Input,
        Disableable, Sizable,
    },
    Theme,
};

pub(super) fn render_reading(
    node_id: NodeId,
    content: AnyElement,
    theme: Theme,
    zoom: f32,
    radius: f32,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .rounded_b(px(radius))
        .bg(rgba(preview_surface_background(theme)))
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .flex_none()
                .px(px(8.0 * zoom))
                .py(px(4.0 * zoom))
                .child(command_button(
                    node_id,
                    "preview-edit",
                    ui::i18n::text("cards.edit"),
                    PreviewEditCommandKind::Edit,
                )),
        )
        .child(content)
        .into_any_element()
}

pub(super) fn render_editor(
    node_id: NodeId,
    editor: &PreviewEditor,
    theme: Theme,
    zoom: f32,
    radius: f32,
) -> AnyElement {
    let status = if editor.saving {
        ui::i18n::text("cards.saving")
    } else if editor.dirty {
        ui::i18n::text("cards.unsaved_changes")
    } else if editor.saved {
        ui::i18n::text("cards.saved")
    } else {
        ui::i18n::text("cards.editing_file")
    };
    div()
        .id(("preview-editor", node_id))
        .debug_selector(|| "preview-editor".to_string())
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .rounded_b(px(radius))
        .bg(rgba(preview_surface_background(theme)))
        // Keep text selection, clicks and spaces inside the editor off the canvas router.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0 * zoom))
                .flex_none()
                .px(px(8.0 * zoom))
                .py(px(4.0 * zoom))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(12.0 * zoom))
                        .text_color(rgba(theme.text_secondary))
                        .child(status),
                )
                .child(
                    command_button(
                        node_id,
                        "preview-cancel",
                        if editor.dirty {
                            ui::i18n::text("common.cancel")
                        } else {
                            "Done"
                        },
                        PreviewEditCommandKind::Cancel,
                    )
                    .disabled(editor.saving),
                )
                .child(
                    command_button(
                        node_id,
                        "preview-save",
                        ui::i18n::text("common.save"),
                        PreviewEditCommandKind::Save,
                    )
                    .primary()
                    .disabled(editor.saving || !editor.dirty),
                ),
        )
        .when_some(editor.error.clone(), |this, error| {
            this.child(
                div()
                    .px(px(12.0 * zoom))
                    .py(px(4.0 * zoom))
                    .text_size(px(12.0 * zoom))
                    .text_color(rgba(theme.destructive))
                    .child(error),
            )
        })
        .child(
            div()
                .flex_1()
                .min_h_0()
                .px(px(8.0 * zoom))
                .pb(px(8.0 * zoom))
                .child(
                    Input::new(&editor.input)
                        .h_full()
                        .font_family("Menlo")
                        .text_size(px(13.0 * zoom))
                        .disabled(editor.saving),
                ),
        )
        .into_any_element()
}

fn command_button(
    node_id: NodeId,
    id: &'static str,
    label: &'static str,
    command: PreviewEditCommandKind,
) -> Button {
    Button::new((id, node_id))
        .debug_selector(move || id.to_string())
        .label(label)
        .small()
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            window.dispatch_action(Box::new(PreviewEditCommand { node_id, command }), cx);
        })
}

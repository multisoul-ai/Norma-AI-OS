//! Preview card renderer.

use crate::cards::chrome::CardChrome;
use crate::cards::{Card, CardBodyContext};
use crate::preview::{MarkdownBlock, MarkdownDocument, PreviewContent, PreviewLoadState};
use canvas::{CanvasNode, CanvasRect, NodeType};
use gpui::{div, img, prelude::*, px, rgba, AnyElement, Div, IntoElement, ObjectFit, Stateful};
use runtime::{NodeResize, Runtime};
use ui::Theme;

const PREVIEW_PADDING_PX: f32 = 12.0;
const PREVIEW_TEXT_PX: f32 = 13.0;
const PREVIEW_LINE_PX: f32 = 19.0;

pub struct PreviewCard;

pub static CARD: PreviewCard = PreviewCard;

impl Card for PreviewCard {
    fn ty(&self) -> NodeType {
        NodeType::Preview
    }

    fn chrome(&self, node: &CanvasNode, _runtime: &Runtime) -> CardChrome {
        CardChrome {
            title: node.kind.display_title(),
            refreshable: true,
            task_badge: None,
        }
    }

    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
        render_preview_state(
            node.id,
            context.preview_state,
            context.theme,
            context.zoom,
            context.bottom_radius_px,
        )
    }

    fn resize(&self, _bounds: CanvasRect, _zoom: f32) -> Option<NodeResize> {
        None
    }

    fn wheel_scroll_mode(&self) -> crate::cards::CardWheelScrollMode {
        crate::cards::CardWheelScrollMode::NativeElement
    }
}

fn render_preview_state(
    node_id: canvas::NodeId,
    state: Option<&PreviewLoadState>,
    theme: Theme,
    zoom: f32,
    bottom_radius_px: f32,
) -> AnyElement {
    match state {
        Some(PreviewLoadState::Ready(PreviewContent::Markdown(document))) => {
            render_markdown(node_id, document, theme, zoom, bottom_radius_px)
        }
        Some(PreviewLoadState::Ready(PreviewContent::Text(text))) => {
            render_text(node_id, text, theme, zoom, bottom_radius_px)
        }
        Some(PreviewLoadState::Ready(PreviewContent::Image { path })) => {
            render_image(node_id, path.clone(), theme, zoom, bottom_radius_px)
        }
        Some(PreviewLoadState::Error(error)) => render_message(
            "Preview error",
            &error.message,
            node_id,
            theme,
            zoom,
            bottom_radius_px,
            true,
        ),
        Some(PreviewLoadState::Loading) | None => render_message(
            "Loading preview",
            "Reading file...",
            node_id,
            theme,
            zoom,
            bottom_radius_px,
            false,
        ),
    }
}

fn render_markdown(
    node_id: canvas::NodeId,
    document: &MarkdownDocument,
    theme: Theme,
    zoom: f32,
    bottom_radius_px: f32,
) -> AnyElement {
    preview_surface(node_id, theme, zoom, bottom_radius_px)
        .children(
            document
                .blocks
                .iter()
                .map(|block| render_markdown_block(block, theme, zoom)),
        )
        .into_any_element()
}

fn render_markdown_block(block: &MarkdownBlock, theme: Theme, zoom: f32) -> AnyElement {
    match block {
        MarkdownBlock::Heading { level, text } => {
            let size = match level {
                1 => 20.0,
                2 => 17.0,
                _ => 15.0,
            } * zoom;
            div()
                .pt(px(if *level == 1 { 2.0 * zoom } else { 6.0 * zoom }))
                .pb(px(2.0 * zoom))
                .text_size(px(size))
                .line_height(px(size * 1.25))
                .text_color(rgba(theme.text_primary))
                .child(text.clone())
                .into_any_element()
        }
        MarkdownBlock::Paragraph(text) => paragraph(text, theme, zoom).into_any_element(),
        MarkdownBlock::ListItem(text) => div()
            .flex()
            .flex_row()
            .gap(px(8.0 * zoom))
            .child(div().text_color(rgba(theme.accent_orange)).child("-"))
            .child(paragraph(text, theme, zoom).flex_1())
            .into_any_element(),
        MarkdownBlock::CodeBlock { language, code } => {
            let label = language.as_deref().unwrap_or("code");
            div()
                .flex()
                .flex_col()
                .gap(px(6.0 * zoom))
                .p(px(10.0 * zoom))
                .rounded(px(7.0 * zoom))
                .bg(rgba(with_alpha(theme.term_bg, 0xcc)))
                .child(
                    div()
                        .text_size(px(10.0 * zoom))
                        .text_color(rgba(theme.text_tertiary))
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .font_family("Menlo")
                        .text_size(px(12.0 * zoom))
                        .line_height(px(17.0 * zoom))
                        .text_color(rgba(theme.term_fg))
                        .child(code.clone()),
                )
                .into_any_element()
        }
        MarkdownBlock::Rule => div()
            .h(px(1.0))
            .w_full()
            .my(px(6.0 * zoom))
            .bg(rgba(with_alpha(theme.edge_highlight, 0x30)))
            .into_any_element(),
    }
}

fn paragraph(text: &str, theme: Theme, zoom: f32) -> Div {
    div()
        .text_size(px(PREVIEW_TEXT_PX * zoom))
        .line_height(px(PREVIEW_LINE_PX * zoom))
        .text_color(rgba(theme.text_secondary))
        .child(text.to_string())
}

fn render_text(
    node_id: canvas::NodeId,
    text: &str,
    theme: Theme,
    zoom: f32,
    bottom_radius_px: f32,
) -> AnyElement {
    preview_surface(node_id, theme, zoom, bottom_radius_px)
        .font_family("Menlo")
        .text_size(px(12.0 * zoom))
        .line_height(px(18.0 * zoom))
        .children(text.lines().map(|line| {
            div().min_h(px(18.0 * zoom)).child(if line.is_empty() {
                " ".to_string()
            } else {
                line.to_string()
            })
        }))
        .into_any_element()
}

fn render_image(
    node_id: canvas::NodeId,
    path: std::path::PathBuf,
    theme: Theme,
    zoom: f32,
    bottom_radius_px: f32,
) -> AnyElement {
    let fallback_theme = theme;
    let loading_theme = theme;
    preview_surface(node_id, theme, zoom, bottom_radius_px)
        .child(
            div().flex_none().child(
                img(path)
                    .object_fit(ObjectFit::Contain)
                    .with_loading(move || {
                        div()
                            .p(px(PREVIEW_PADDING_PX * zoom))
                            .text_color(rgba(loading_theme.text_tertiary))
                            .child("Loading image...")
                            .into_any_element()
                    })
                    .with_fallback(move || {
                        div()
                            .p(px(PREVIEW_PADDING_PX * zoom))
                            .text_color(rgba(fallback_theme.destructive))
                            .child("Could not decode image")
                            .into_any_element()
                    }),
            ),
        )
        .into_any_element()
}

fn render_message(
    title: &str,
    detail: &str,
    node_id: canvas::NodeId,
    theme: Theme,
    zoom: f32,
    bottom_radius_px: f32,
    error: bool,
) -> AnyElement {
    let accent = if error {
        theme.destructive
    } else {
        theme.text_tertiary
    };
    preview_surface(node_id, theme, zoom, bottom_radius_px)
        .items_center()
        .justify_center()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.0 * zoom))
                .text_color(rgba(theme.text_secondary))
                .child(
                    div()
                        .text_color(rgba(accent))
                        .text_size(px(13.0 * zoom))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0 * zoom))
                        .text_color(rgba(theme.text_tertiary))
                        .child(detail.to_string()),
                ),
        )
        .into_any_element()
}

fn preview_surface(
    node_id: canvas::NodeId,
    theme: Theme,
    zoom: f32,
    bottom_radius_px: f32,
) -> Stateful<Div> {
    div()
        .id(("preview-content", node_id))
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .gap(px(9.0 * zoom))
        .p(px(PREVIEW_PADDING_PX * zoom))
        .rounded_b(px(bottom_radius_px))
        .overflow_scroll()
        .bg(rgba(with_alpha(theme.term_bg, 0x52)))
        .text_color(rgba(theme.text_secondary))
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

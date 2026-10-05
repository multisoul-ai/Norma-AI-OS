//! Preview card renderer.

use crate::cards::chrome::CardChrome;
use crate::cards::preview::image_viewer::render_image_viewer;
use crate::cards::{Card, CardBodyContext};
use crate::preview::{
    ImageViewerPresentation, MarkdownBlock, MarkdownDocument, PreviewContent, PreviewLoadState,
};
use canvas::{CanvasNode, CanvasRect, NodeType};
use gpui::{div, prelude::*, px, rgba, AnyElement, Div, IntoElement, Stateful};
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
            title: crate::cards::title::display_title(&node.kind),
            title_detail: None,
            refreshable: true,
            task_badge: None,
        }
    }

    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
        if let Some(editor) = context.preview_editor {
            return super::editor::render_editor(
                node.id,
                editor,
                context.theme,
                context.zoom,
                context.bottom_radius_px,
            );
        }
        let editable = matches!(
            context.preview_state,
            Some(PreviewLoadState::Ready(content)) if content.text_source().is_some()
        );
        let theme = context.theme;
        let zoom = context.zoom;
        let bottom_radius = context.bottom_radius_px;
        let content = render_preview_state(
            node.id,
            context.preview_state,
            context.preview_image_viewer,
            context.preview_image_dragging,
            context.theme,
            context.zoom,
            context.bottom_radius_px,
        );
        if editable {
            super::editor::render_reading(node.id, content, theme, zoom, bottom_radius)
        } else {
            content
        }
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
    image_viewer: Option<ImageViewerPresentation>,
    image_dragging: bool,
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
        Some(PreviewLoadState::Ready(PreviewContent::Image { path })) => render_loaded_images(
            node_id,
            std::slice::from_ref(path),
            image_viewer,
            image_dragging,
            theme,
            zoom,
            bottom_radius_px,
        ),
        Some(PreviewLoadState::Ready(PreviewContent::Images { paths })) => render_loaded_images(
            node_id,
            paths,
            image_viewer,
            image_dragging,
            theme,
            zoom,
            bottom_radius_px,
        ),
        Some(PreviewLoadState::Error(error)) => render_message(
            ui::i18n::text("cards.preview_error"),
            &error.message,
            node_id,
            theme,
            zoom,
            bottom_radius_px,
            true,
        ),
        Some(PreviewLoadState::Loading) | None => render_message(
            ui::i18n::text("cards.loading_preview"),
            ui::i18n::text("cards.reading_file"),
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

fn render_loaded_images(
    node_id: canvas::NodeId,
    paths: &[std::path::PathBuf],
    image_viewer: Option<ImageViewerPresentation>,
    dragging: bool,
    theme: Theme,
    zoom: f32,
    bottom_radius_px: f32,
) -> AnyElement {
    let Some(image_viewer) = image_viewer else {
        return render_message(
            ui::i18n::text("cards.loading_image_2"),
            ui::i18n::text("cards.preparing_viewer"),
            node_id,
            theme,
            zoom,
            bottom_radius_px,
            false,
        );
    };
    render_image_viewer(
        node_id,
        paths,
        image_viewer,
        dragging,
        theme,
        zoom,
        bottom_radius_px,
    )
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
        .bg(rgba(preview_surface_background(theme)))
        .text_color(rgba(theme.text_secondary))
}

pub(super) fn preview_surface_background(theme: Theme) -> u32 {
    with_alpha(theme.panel_high, 0xf2)
}

pub(super) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preview 阅读面应使用高不透明的 panel_high，避免壁纸穿透后破坏正文对比度。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   Paper Blue panel_high = 0xfffefbd1
    ///   阅读面目标 alpha       = 0xf2（242 / 255 ≈ 94.9%）
    ///   旧实现 alpha           = 0x52（82 / 255 ≈ 32.2%）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造 Paper Blue 主题。
    ///   2. 解析 Preview 正文阅读面的背景色。
    ///   3. 分别检查 RGB 来源、目标 alpha 与旧半透明背景的差异。
    ///
    /// 预期结果：
    ///   - 正断言：RGB 必须来自 panel_high，保持主题一致性。
    ///   - 正断言：alpha 必须精确为 0xf2，形成稳定阅读底色。
    ///   - 负断言：不得回退到 term_bg + 0x52 的高穿透背景。
    #[test]
    fn preview_surface_background_is_theme_aware_and_high_opacity() {
        let theme = Theme::paper_blue();

        let background = preview_surface_background(theme);

        assert_eq!(
            background & 0xffffff00,
            theme.panel_high & 0xffffff00,
            "Preview 阅读面 RGB 应来自 panel_high，以匹配当前主题"
        );
        assert_eq!(
            background & 0xff,
            0xf2,
            "Preview 阅读面 alpha 应为 0xf2（约 94.9%），确保壁纸不会干扰正文"
        );
        assert_ne!(
            background,
            with_alpha(theme.term_bg, 0x52),
            "Preview 阅读面不得回退到旧的 term_bg + 0x52 高穿透背景"
        );
    }
}

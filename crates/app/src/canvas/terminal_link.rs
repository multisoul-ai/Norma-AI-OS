use crate::canvas::text_selection::TextPoint;
use gpui::{div, prelude::*, px, rgba, AnyElement};
use terminal::{GridSnapshot, PathRef, PathTier};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TerminalLinkIntent {
    OpenBrowserCard { url: String },
    OpenDefaultBrowser { url: String },
    OpenPath { paths: Vec<String>, system: bool },
    InvalidLink,
    BeginTextSelection,
    Ignore,
}

/// `verified` 报告某个待验证候选是否已确认存在；与下划线共用同一份路径检测，保证有下划线就能点。
pub(crate) fn terminal_link_intent(
    snapshot: &GridSnapshot,
    point: TextPoint,
    platform_modifier: bool,
    click_count: usize,
    verified: &dyn Fn(&PathRef) -> bool,
) -> TerminalLinkIntent {
    if click_count != 1 {
        return TerminalLinkIntent::Ignore;
    }
    let Some(url) = snapshot.link_at(point.row, point.col) else {
        let mut paths = snapshot.preview_path_candidates_at(point.row, point.col);
        if paths.is_empty() {
            if let Some(path_ref) = snapshot.path_ref_at(point.row, point.col) {
                if path_ref.tier == PathTier::Strong || verified(&path_ref) {
                    paths.push(path_ref.raw);
                }
            }
        }
        if !paths.is_empty() {
            return TerminalLinkIntent::OpenPath {
                paths,
                system: platform_modifier,
            };
        }
        return if snapshot.has_link_candidate_at(point.row, point.col) {
            TerminalLinkIntent::InvalidLink
        } else {
            TerminalLinkIntent::BeginTextSelection
        };
    };
    if platform_modifier {
        TerminalLinkIntent::OpenDefaultBrowser { url }
    } else {
        TerminalLinkIntent::OpenBrowserCard { url }
    }
}

pub(crate) fn render_terminal_link_tooltip(
    link: crate::canvas::interaction::HoveredTerminalLink,
    theme: ui::Theme,
    window_width: f32,
    window_height: f32,
) -> AnyElement {
    let left = (link.screen_x as f32 + 12.0).clamp(8.0, (window_width - 360.0).max(8.0));
    let top = (link.screen_y as f32 + 18.0).clamp(8.0, (window_height - 34.0).max(8.0));
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .max_w(px(352.0))
        .px(px(8.0))
        .py(px(5.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.accent_cyan, 0x72)))
        .bg(rgba(with_alpha(theme.panel, 0xf2)))
        .text_size(px(11.0))
        .text_color(rgba(theme.text_primary))
        .overflow_hidden()
        .truncate()
        .child(link.url)
        .into_any_element()
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../../tests/unit/terminal_link_tests.rs"]
mod tests;

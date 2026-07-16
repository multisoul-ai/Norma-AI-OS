use crate::canvas::text_selection::TextPoint;
use terminal::GridSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TerminalLinkIntent {
    OpenBrowserCard { url: String },
    OpenDefaultBrowser { url: String },
    OpenPreviewCard { path: String },
    InvalidLink,
    BeginTextSelection,
    Ignore,
}

pub(crate) fn terminal_link_intent(
    snapshot: &GridSnapshot,
    point: TextPoint,
    platform_modifier: bool,
    click_count: usize,
) -> TerminalLinkIntent {
    if click_count != 1 {
        return TerminalLinkIntent::Ignore;
    }
    let Some(url) = snapshot.link_at(point.row, point.col) else {
        if let Some(path) = snapshot.preview_path_at(point.row, point.col) {
            return TerminalLinkIntent::OpenPreviewCard { path };
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

#[cfg(test)]
#[path = "../../tests/unit/terminal_link_tests.rs"]
mod tests;

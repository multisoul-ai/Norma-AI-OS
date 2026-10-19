//! Pure width split for the file workbench: document on the left, compact tree on the right.
//!
//! Mirrors the prototype's `grid-template-columns:minmax(0,1fr) minmax(170px,32%)`.
//! The prototype's `@media(max-width:520px)` targets the browser viewport, so the drawer
//! only goes compact when the document column itself would be too narrow to read.

pub(crate) const MIN_SIDEBAR_WIDTH: f32 = 170.0;
pub(crate) const SIDEBAR_FRACTION: f32 = 0.32;
pub(crate) const MIN_DOCUMENT_WIDTH: f32 = 200.0;
pub(crate) const DIVIDER_WIDTH: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExplorerLayout {
    pub sidebar: f32,
    pub document: f32,
    pub compact: bool,
}

pub(crate) fn explorer_layout(width: f32) -> ExplorerLayout {
    let sidebar = (width * SIDEBAR_FRACTION).max(MIN_SIDEBAR_WIDTH);
    let document = (width - sidebar - DIVIDER_WIDTH).max(0.0);
    ExplorerLayout {
        sidebar,
        document,
        compact: document < MIN_DOCUMENT_WIDTH,
    }
}

use canvas::{NodeId, PreviewType};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod actions;
mod editing;
mod editing_actions;
pub(crate) use editing::{PreviewEditCommand, PreviewEditCommandKind, PreviewEditor};
mod image_viewer;
mod terminal_path;
use image_viewer::ImageViewerSession;
#[cfg(test)]
pub(crate) use image_viewer::{drag_scroll_offset, ImageViewerState};
pub(crate) use image_viewer::{
    ImageViewerPresentation, PreviewImageCommand, PreviewImageCommandKind,
};

#[derive(Default)]
pub(crate) struct PreviewStore {
    states: HashMap<NodeId, PreviewLoadState>,
    generations: HashMap<NodeId, u64>,
    image_viewers: HashMap<NodeId, ImageViewerSession>,
    editors: HashMap<NodeId, PreviewEditor>,
    next_generation: u64,
}

impl PreviewStore {
    pub(crate) fn state_for(&self, node_id: NodeId) -> Option<&PreviewLoadState> {
        self.states.get(&node_id)
    }

    fn begin_loading(&mut self, node_id: NodeId) -> u64 {
        self.next_generation = self.next_generation.saturating_add(1);
        let generation = self.next_generation;
        self.generations.insert(node_id, generation);
        self.states.insert(node_id, PreviewLoadState::Loading);
        generation
    }

    fn finish_loading(
        &mut self,
        node_id: NodeId,
        generation: u64,
        result: Result<PreviewContent, PreviewReadError>,
    ) -> bool {
        if self.generations.get(&node_id).copied() != Some(generation) {
            return false;
        }

        let state = match result {
            Ok(content) => {
                let image_count = match &content {
                    PreviewContent::Image { .. } => Some(1),
                    PreviewContent::Images { paths } => Some(paths.len()),
                    PreviewContent::Markdown(_) | PreviewContent::Text(_) => None,
                };
                self.reconcile_image_viewer(node_id, image_count);
                PreviewLoadState::Ready(content)
            }
            Err(error) => PreviewLoadState::Error(PreviewError {
                message: error.to_string(),
            }),
        };
        self.states.insert(node_id, state);
        true
    }

    pub(crate) fn remove(&mut self, node_id: NodeId) {
        self.states.remove(&node_id);
        self.generations.remove(&node_id);
        self.image_viewers.remove(&node_id);
        self.editors.remove(&node_id);
    }

    pub(crate) fn clear(&mut self) {
        self.states.clear();
        self.generations.clear();
        self.image_viewers.clear();
        self.editors.clear();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreviewLoadState {
    Loading,
    Ready(PreviewContent),
    Error(PreviewError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreviewContent {
    Markdown(MarkdownDocument),
    Text(String),
    Image { path: PathBuf },
    Images { paths: Vec<PathBuf> },
}

impl PreviewContent {
    pub(crate) fn text_source(&self) -> Option<&str> {
        match self {
            Self::Markdown(document) => Some(&document.source),
            Self::Text(text) => Some(text),
            Self::Image { .. } | Self::Images { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarkdownDocument {
    pub(crate) source: String,
    pub(crate) blocks: Vec<MarkdownBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MarkdownBlock {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph(String),
    ListItem(String),
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    Rule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreviewError {
    pub(crate) message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreviewReadError {
    EmptySelection,
    NotFound,
    NotAFile,
    UnsupportedType,
    MultipleSelectionRequiresImages,
    ReadFailed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreviewPathError {
    InvalidFileUri,
}

impl std::fmt::Display for PreviewPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFileUri => write!(f, "Invalid local file URI"),
        }
    }
}

impl std::fmt::Display for PreviewReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptySelection => write!(f, "Select at least one preview file"),
            Self::NotFound => write!(f, "File does not exist"),
            Self::NotAFile => write!(f, "Path is not a file"),
            Self::UnsupportedType => write!(f, "Unsupported preview type"),
            Self::MultipleSelectionRequiresImages => {
                write!(f, "Multiple selection supports image files only")
            }
            Self::ReadFailed(error) => write!(f, "Could not read file: {error}"),
        }
    }
}

pub(crate) fn resolve_preview_path(
    workspace_root: &Path,
    target: &str,
) -> Result<PathBuf, PreviewPathError> {
    if target.starts_with("file://") {
        return url::Url::parse(target)
            .ok()
            .filter(|url| url.scheme() == "file")
            .and_then(|url| url.to_file_path().ok())
            .ok_or(PreviewPathError::InvalidFileUri);
    }

    let path = PathBuf::from(target);
    Ok(if path.is_absolute() {
        path
    } else {
        workspace_root.join(path)
    })
}

pub(crate) fn infer_preview_type(
    path: &Path,
    explicit_type: Option<PreviewType>,
) -> Result<PreviewType, PreviewReadError> {
    if let Some(preview_type) = explicit_type {
        return Ok(preview_type);
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .ok_or(PreviewReadError::UnsupportedType)?;

    match extension.as_str() {
        "md" | "markdown" | "mdown" | "mkd" => Ok(PreviewType::Markdown),
        "txt" | "text" | "log" => Ok(PreviewType::Text),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "svg" => {
            Ok(PreviewType::Image)
        }
        _ => Err(PreviewReadError::UnsupportedType),
    }
}

pub(crate) fn load_preview_content(
    path: PathBuf,
    explicit_type: Option<PreviewType>,
) -> Result<PreviewContent, PreviewReadError> {
    if !path.exists() {
        return Err(PreviewReadError::NotFound);
    }
    if !path.is_file() {
        return Err(PreviewReadError::NotAFile);
    }

    match infer_preview_type(&path, explicit_type)? {
        PreviewType::Markdown => {
            let source = read_text_file(&path)?;
            let blocks = parse_markdown_blocks(&source);
            Ok(PreviewContent::Markdown(MarkdownDocument {
                source,
                blocks,
            }))
        }
        PreviewType::Text => Ok(PreviewContent::Text(read_text_file(&path)?)),
        PreviewType::Image => Ok(PreviewContent::Image { path }),
    }
}

pub(crate) fn load_preview_contents(
    paths: Vec<PathBuf>,
    explicit_type: Option<PreviewType>,
) -> Result<PreviewContent, PreviewReadError> {
    if paths.is_empty() {
        return Err(PreviewReadError::EmptySelection);
    }
    if paths.len() == 1 {
        let Some(path) = paths.into_iter().next() else {
            return Err(PreviewReadError::EmptySelection);
        };
        return load_preview_content(path, explicit_type);
    }

    for path in &paths {
        if !path.exists() {
            return Err(PreviewReadError::NotFound);
        }
        if !path.is_file() {
            return Err(PreviewReadError::NotAFile);
        }
        if infer_preview_type(path, explicit_type).ok() != Some(PreviewType::Image) {
            return Err(PreviewReadError::MultipleSelectionRequiresImages);
        }
    }

    Ok(PreviewContent::Images { paths })
}

fn read_text_file(path: &Path) -> Result<String, PreviewReadError> {
    std::fs::read_to_string(path).map_err(|error| PreviewReadError::ReadFailed(error.to_string()))
}

fn parse_markdown_blocks(source: &str) -> Vec<MarkdownBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut blocks = Vec::new();
    let mut current: Option<MarkdownAccumulator> = None;
    let mut list_depth = 0usize;

    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(Tag::Paragraph) => {
                if current.is_none() {
                    current = Some(MarkdownAccumulator::Paragraph(String::new()));
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if matches!(current, Some(MarkdownAccumulator::Paragraph(_))) {
                    finish_accumulator(&mut blocks, current.take());
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some(MarkdownAccumulator::Heading {
                    level: heading_level(level),
                    text: String::new(),
                });
            }
            Event::End(TagEnd::Heading(_)) => finish_accumulator(&mut blocks, current.take()),
            Event::Start(Tag::List(_)) => {
                list_depth = list_depth.saturating_add(1);
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
            }
            Event::Start(Tag::Item) => {
                current = Some(MarkdownAccumulator::ListItem(String::new()));
            }
            Event::End(TagEnd::Item) => finish_accumulator(&mut blocks, current.take()),
            Event::Start(Tag::CodeBlock(kind)) => {
                current = Some(MarkdownAccumulator::CodeBlock {
                    language: code_block_language(kind),
                    code: String::new(),
                });
            }
            Event::End(TagEnd::CodeBlock) => finish_accumulator(&mut blocks, current.take()),
            Event::Rule => blocks.push(MarkdownBlock::Rule),
            Event::Text(text) | Event::Code(text) | Event::Html(text) => {
                append_markdown_text(&mut current, list_depth, &text);
            }
            Event::SoftBreak => append_markdown_break(&mut current, false),
            Event::HardBreak => append_markdown_break(&mut current, true),
            Event::TaskListMarker(checked) => {
                append_markdown_text(
                    &mut current,
                    list_depth,
                    if checked { "[x] " } else { "[ ] " },
                );
            }
            _ => {}
        }
    }
    finish_accumulator(&mut blocks, current);
    blocks
}

#[derive(Debug)]
enum MarkdownAccumulator {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph(String),
    ListItem(String),
    CodeBlock {
        language: Option<String>,
        code: String,
    },
}

fn append_markdown_text(current: &mut Option<MarkdownAccumulator>, list_depth: usize, text: &str) {
    if current.is_none() {
        *current = if list_depth > 0 {
            Some(MarkdownAccumulator::ListItem(String::new()))
        } else {
            Some(MarkdownAccumulator::Paragraph(String::new()))
        };
    }

    match current.as_mut().expect("current accumulator should exist") {
        MarkdownAccumulator::Heading { text: target, .. }
        | MarkdownAccumulator::Paragraph(target)
        | MarkdownAccumulator::ListItem(target) => target.push_str(text),
        MarkdownAccumulator::CodeBlock { code, .. } => code.push_str(text),
    }
}

fn append_markdown_break(current: &mut Option<MarkdownAccumulator>, hard: bool) {
    let Some(current) = current.as_mut() else {
        return;
    };
    let separator = match current {
        MarkdownAccumulator::CodeBlock { .. } | MarkdownAccumulator::ListItem(_) if hard => "\n",
        MarkdownAccumulator::CodeBlock { .. } => "\n",
        _ => " ",
    };
    match current {
        MarkdownAccumulator::Heading { text, .. }
        | MarkdownAccumulator::Paragraph(text)
        | MarkdownAccumulator::ListItem(text) => text.push_str(separator),
        MarkdownAccumulator::CodeBlock { code, .. } => code.push_str(separator),
    }
}

fn finish_accumulator(blocks: &mut Vec<MarkdownBlock>, accumulator: Option<MarkdownAccumulator>) {
    let Some(accumulator) = accumulator else {
        return;
    };
    match accumulator {
        MarkdownAccumulator::Heading { level, text } => {
            let text = text.trim();
            if !text.is_empty() {
                blocks.push(MarkdownBlock::Heading {
                    level,
                    text: text.to_string(),
                });
            }
        }
        MarkdownAccumulator::Paragraph(text) => {
            let text = text.trim();
            if !text.is_empty() {
                blocks.push(MarkdownBlock::Paragraph(text.to_string()));
            }
        }
        MarkdownAccumulator::ListItem(text) => {
            let text = text.trim();
            if !text.is_empty() {
                blocks.push(MarkdownBlock::ListItem(text.to_string()));
            }
        }
        MarkdownAccumulator::CodeBlock { language, code } => {
            blocks.push(MarkdownBlock::CodeBlock { language, code });
        }
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn code_block_language(kind: CodeBlockKind<'_>) -> Option<String> {
    match kind {
        CodeBlockKind::Indented => None,
        CodeBlockKind::Fenced(language) => {
            let language = language.trim();
            if language.is_empty() {
                None
            } else {
                Some(language.to_string())
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/preview_tests.rs"]
mod tests;

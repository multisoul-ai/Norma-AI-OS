use crate::Runtime;
use canvas::{CanvasNodeKind, CanvasRect, NodeId, PreviewSpec, PreviewType};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewSpawnError {
    EmptyPaths,
}

impl std::fmt::Display for PreviewSpawnError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPaths => write!(formatter, "Preview requires at least one path"),
        }
    }
}

impl std::error::Error for PreviewSpawnError {}

impl Runtime {
    pub fn spawn_preview_from_path(
        &mut self,
        path: PathBuf,
        explicit_type: Option<PreviewType>,
        bounds: CanvasRect,
    ) -> NodeId {
        self.add_preview_node(path, Vec::new(), explicit_type, bounds)
    }

    pub fn spawn_preview_from_paths(
        &mut self,
        paths: Vec<PathBuf>,
        explicit_type: Option<PreviewType>,
        bounds: CanvasRect,
    ) -> Result<NodeId, PreviewSpawnError> {
        let mut paths = paths.into_iter();
        let Some(path) = paths.next() else {
            return Err(PreviewSpawnError::EmptyPaths);
        };
        let additional_paths = paths
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        Ok(self.add_preview_node(path, additional_paths, explicit_type, bounds))
    }

    fn add_preview_node(
        &mut self,
        path: PathBuf,
        additional_paths: Vec<String>,
        explicit_type: Option<PreviewType>,
        bounds: CanvasRect,
    ) -> NodeId {
        self.document.add_node(
            CanvasNodeKind::Preview(PreviewSpec {
                path: path.to_string_lossy().into_owned(),
                additional_paths,
                explicit_type,
            }),
            bounds,
        )
    }
}

use super::state::WallpaperSelection;
use storage::{DesktopProjectArea, DesktopWorkspace, WallpaperReference, WallpaperUsage};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WallpaperTarget {
    Global,
    Workspace(String),
    Project {
        workspace_id: String,
        area_id: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct WallpaperContext {
    pub workspace: DesktopWorkspace,
    pub area: Option<DesktopProjectArea>,
}

#[derive(Clone, Debug)]
pub(crate) struct WallpaperDeletePrompt {
    pub filename: String,
    pub usage: WallpaperUsage,
}

/// Cached inputs only: resolving an effective selection never reads SQLite or scans files.
#[derive(Clone, Debug)]
pub(crate) struct ScopedWallpaperState {
    pub context: Option<WallpaperContext>,
    pub edit_project: bool,
    pub global: WallpaperSelection,
    pub customs: Vec<String>,
    pub busy: bool,
    pub revision: u64,
    pub status: Option<String>,
    pub delete_prompt: Option<WallpaperDeletePrompt>,
}

impl ScopedWallpaperState {
    pub(crate) fn new(global: WallpaperSelection, customs: Vec<String>) -> Self {
        Self {
            context: None,
            edit_project: false,
            global,
            customs,
            busy: false,
            revision: 0,
            status: None,
            delete_prompt: None,
        }
    }

    pub(crate) fn bind(&mut self, context: Option<WallpaperContext>) {
        let changed = self.context.as_ref().map(context_key) != context.as_ref().map(context_key);
        if changed {
            self.edit_project = context
                .as_ref()
                .is_some_and(|context| context.area.is_some());
            self.status = None;
        }
        self.context = context;
    }

    pub(crate) fn target(&self) -> WallpaperTarget {
        let Some(context) = &self.context else {
            return WallpaperTarget::Global;
        };
        if self.edit_project {
            if let Some(area) = &context.area {
                return WallpaperTarget::Project {
                    workspace_id: context.workspace.id.clone(),
                    area_id: area.id.clone(),
                };
            }
        }
        WallpaperTarget::Workspace(context.workspace.id.clone())
    }

    pub(crate) fn saved_reference(&self) -> Option<WallpaperReference> {
        match self.target() {
            WallpaperTarget::Global => Some(self.global.reference()),
            WallpaperTarget::Workspace(_) => self.context.as_ref()?.workspace.wallpaper.clone(),
            WallpaperTarget::Project { .. } => {
                self.context.as_ref()?.area.as_ref()?.wallpaper.clone()
            }
        }
    }

    pub(crate) fn resolve(&self, reference: &WallpaperReference) -> Option<WallpaperSelection> {
        reference.validate().ok()?;
        match reference {
            WallpaperReference::Builtin { rel } => {
                crate::app::wallpaper_resolvers::canonical_builtin_wallpaper(rel)
                    .map(|rel| WallpaperSelection::Builtin { rel })
            }
            WallpaperReference::Custom { filename } => {
                self.customs
                    .contains(filename)
                    .then(|| WallpaperSelection::Custom {
                        filename: filename.clone(),
                    })
            }
        }
    }

    pub(crate) fn workspace_selection(&self) -> WallpaperSelection {
        self.context
            .as_ref()
            .and_then(|context| context.workspace.wallpaper.as_ref())
            .and_then(|reference| self.resolve(reference))
            .unwrap_or_default()
    }

    pub(crate) fn effective_selection(&self) -> WallpaperSelection {
        let Some(context) = &self.context else {
            return self.global.clone();
        };
        context
            .area
            .as_ref()
            .and_then(|area| area.wallpaper.as_ref())
            .and_then(|reference| self.resolve(reference))
            .unwrap_or_else(|| self.workspace_selection())
    }

    pub(crate) fn edit_selection(&self) -> WallpaperSelection {
        if self.context.is_some() && !self.edit_project {
            self.workspace_selection()
        } else {
            self.effective_selection()
        }
    }

    pub(crate) fn follows_workspace(&self) -> bool {
        self.context
            .as_ref()
            .and_then(|context| context.area.as_ref())
            .is_some_and(|area| area.wallpaper.is_none())
    }

    pub(crate) fn has_missing_selection(&self) -> bool {
        let missing = |reference: &WallpaperReference| self.resolve(reference).is_none();
        self.context.as_ref().is_some_and(|context| {
            context.workspace.wallpaper.as_ref().is_some_and(missing)
                || context
                    .area
                    .as_ref()
                    .and_then(|area| area.wallpaper.as_ref())
                    .is_some_and(missing)
        })
    }

    pub(crate) fn apply(
        &mut self,
        target: &WallpaperTarget,
        reference: Option<WallpaperReference>,
    ) {
        match target {
            WallpaperTarget::Global => {
                self.global = reference
                    .as_ref()
                    .and_then(|reference| self.resolve(reference))
                    .unwrap_or_default()
            }
            WallpaperTarget::Workspace(id) => {
                if let Some(context) = self
                    .context
                    .as_mut()
                    .filter(|context| &context.workspace.id == id)
                {
                    context.workspace.wallpaper = reference;
                }
            }
            WallpaperTarget::Project {
                workspace_id,
                area_id,
            } => {
                if let Some(area) = self
                    .context
                    .as_mut()
                    .filter(|context| &context.workspace.id == workspace_id)
                    .and_then(|context| context.area.as_mut())
                    .filter(|area| &area.id == area_id)
                {
                    area.wallpaper = reference;
                }
            }
        }
    }
}

fn context_key(context: &WallpaperContext) -> (&str, Option<&str>) {
    (
        &context.workspace.id,
        context.area.as_ref().map(|area| area.id.as_str()),
    )
}

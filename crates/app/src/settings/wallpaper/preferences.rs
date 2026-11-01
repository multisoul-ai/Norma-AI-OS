use super::state::WallpaperSelection;
use storage::WallpaperReference;

#[derive(Clone, Debug)]
pub(crate) struct WallpaperDeletePrompt {
    pub filename: String,
}

/// Global selection and cached library; render paths never query storage.
#[derive(Clone, Debug)]
pub(crate) struct WallpaperPreferences {
    pub global: WallpaperSelection,
    pub customs: Vec<String>,
    pub busy: bool,
    pub revision: u64,
    pub status: Option<ui::i18n::LocalizedText>,
    pub delete_prompt: Option<WallpaperDeletePrompt>,
}

impl WallpaperPreferences {
    pub(crate) fn new(global: WallpaperSelection, customs: Vec<String>) -> Self {
        Self {
            global,
            customs,
            busy: false,
            revision: 0,
            status: None,
            delete_prompt: None,
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

    pub(crate) fn effective_selection(&self) -> WallpaperSelection {
        self.resolve(&self.global.reference()).unwrap_or_default()
    }
}

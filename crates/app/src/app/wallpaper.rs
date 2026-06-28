use crate::app::MultisoulApp;
use crate::assets::DEFAULT_WALLPAPER_ASSET;
use gpui::ImageSource;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceWallpaper {
    Embedded(&'static str),
    File(PathBuf),
}

impl WorkspaceWallpaper {
    pub(crate) fn image_source(&self) -> ImageSource {
        match self {
            Self::Embedded(asset_path) => (*asset_path).into(),
            Self::File(path) => path.clone().into(),
        }
    }
}

impl MultisoulApp {
    pub(crate) fn wallpaper_image_source(&self) -> ImageSource {
        self.wallpaper.image_source()
    }
}

pub(crate) fn wallpaper_from_env() -> WorkspaceWallpaper {
    wallpaper_from_env_value(std::env::var_os("MULTISOUL_WALLPAPER"))
}

pub(crate) fn wallpaper_from_env_value(env_value: Option<OsString>) -> WorkspaceWallpaper {
    let Some(path) = env_value.map(PathBuf::from) else {
        return WorkspaceWallpaper::Embedded(DEFAULT_WALLPAPER_ASSET);
    };

    if path.is_file() {
        WorkspaceWallpaper::File(path)
    } else {
        tracing::warn!(
            path = %path.display(),
            default_asset = DEFAULT_WALLPAPER_ASSET,
            "MULTISOUL_WALLPAPER does not point to a readable file; using embedded default wallpaper"
        );
        WorkspaceWallpaper::Embedded(DEFAULT_WALLPAPER_ASSET)
    }
}

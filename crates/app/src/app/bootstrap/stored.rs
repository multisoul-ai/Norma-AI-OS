//! 可在后台准备的持久设置；构建项目实体时不重复读取文件和 SQLite。
use super::*;

pub(crate) struct StoredBootstrap {
    pub(super) wallpaper: WallpaperSettingsState,
    pub(super) workspace: WorkspaceState,
    pub(super) desktop: crate::workspace::desktop::DesktopWorkspaceUiState,
    pub(super) theme_id: ui::ThemeId,
    pub(super) theme: ThemeSettingsState,
    pub(super) performance: PerformanceSettingsState,
    pub(super) voice: crate::settings::voice::VoiceSettingsState,
    pub(super) shortcuts: ShortcutSettingsState,
    pub(super) guide: super::super::feature_guide::FeatureGuideState,
    pub(super) guide_settings: FeatureGuideSettingsState,
}

impl StoredBootstrap {
    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(crate) fn in_memory(store: &storage::AppStore) -> Self {
        let (theme_id, theme) = ThemeSettingsState::in_memory_default();
        let (guide, guide_settings) = FeatureGuideSettingsState::in_memory_suppressed();
        Self {
            wallpaper: WallpaperSettingsState::load_from_store(store.settings_store()),
            workspace: WorkspaceState::load_from_storage(store.recent_store()),
            desktop: crate::workspace::desktop::DesktopWorkspaceUiState::load(
                store.desktop_workspace_store(),
            ),
            theme_id,
            theme,
            performance: PerformanceSettingsState::in_memory_default(),
            voice: crate::settings::voice::VoiceSettingsState::in_memory_default(),
            shortcuts: ShortcutSettingsState::default(),
            guide,
            guide_settings,
        }
    }

    pub(crate) fn load(store: &storage::AppStore) -> Self {
        let settings = store.settings_store();
        let workspace = WorkspaceState::load_from_storage(store.recent_store());
        let (theme_id, theme) = ThemeSettingsState::load_from_store(settings.clone());
        let (guide, guide_settings) = FeatureGuideSettingsState::load_from_store(
            settings.clone(),
            !workspace.recent.records.is_empty(),
        );
        Self {
            wallpaper: WallpaperSettingsState::load_from_store(settings.clone()),
            workspace,
            desktop: crate::workspace::desktop::DesktopWorkspaceUiState::load(
                store.desktop_workspace_store(),
            ),
            theme_id,
            theme,
            performance: PerformanceSettingsState::load_from_store(settings.clone()),
            voice: crate::settings::voice::VoiceSettingsState::load_from_store(settings.clone()),
            shortcuts: ShortcutSettingsState::load_from_store(settings),
            guide,
            guide_settings,
        }
    }
}

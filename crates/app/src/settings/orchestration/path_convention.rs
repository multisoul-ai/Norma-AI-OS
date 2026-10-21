//! “编排”设置里的文件路径输出约定开关：开启时 Norma 向其启动的 Agent 注入路径输出约定。

use super::with_alpha;
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use storage::SettingsStore;
use ui::component::switch::Switch;
use ui::component::{Sizable as _, Size};
use ui::Theme;

const PATH_CONVENTION_KEY: &str = "orchestration.path_convention.v1";

/// 读取开关；缺省或无法识别时按开启处理，并同步到 runtime（只影响之后启动的 Agent）。
pub(crate) fn load_path_convention(store: &SettingsStore) -> bool {
    let enabled = match store.get_string(PATH_CONVENTION_KEY) {
        Ok(Some(saved)) => saved != "false",
        Ok(None) => true,
        Err(error) => {
            tracing::warn!(error = %error, "failed to load path convention setting");
            true
        }
    };
    runtime::set_path_convention_enabled(enabled);
    enabled
}

impl MultisoulApp {
    pub(crate) fn set_path_convention(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.orchestration_settings.path_convention_enabled = enabled;
        runtime::set_path_convention_enabled(enabled);
        if let Some(store) = self.orchestration_settings.settings_store.clone() {
            // 写库放后台，前台只更新状态。
            cx.background_executor()
                .spawn(async move {
                    if let Err(error) = store
                        .set_string(PATH_CONVENTION_KEY, if enabled { "true" } else { "false" })
                    {
                        tracing::warn!(error = %error, "failed to persist path convention setting");
                    }
                })
                .detach();
        }
        cx.notify();
    }
}

pub(super) fn path_convention_row(
    theme: Theme,
    enabled: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let app = cx.weak_entity();
    div()
        .mr(px(super::PROVIDER_SECTION_RIGHT_INSET))
        .py(px(14.0))
        .flex()
        .items_center()
        .gap(px(16.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .settings_text(TextRole::RowTitle, theme)
                        .child(ui::i18n::text("settings.path_convention_title")),
                )
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(ui::i18n::text("settings.path_convention_hint")),
                ),
        )
        .child(
            div()
                .id("settings-path-convention")
                .debug_selector(|| "settings-path-convention".to_string())
                .w(px(36.0))
                .h(px(20.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Switch::new("settings-path-convention-switch")
                        .checked(enabled)
                        .with_size(Size::Medium)
                        .on_click(move |checked, _window, cx| {
                            let _ = app.update(cx, |app, cx| {
                                app.set_path_convention(*checked, cx);
                            });
                        }),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 路径约定开关的读取：缺省开启；保存 "false" 后关闭；未知值按开启；读取后同步到 runtime。
    ///
    /// 数据构造：隔离 SQLite 的 SettingsStore。
    /// 执行过程：空库加载 → 写入 "false" 再加载 → 写入乱值再加载。
    /// 预期结果：依次为 true / false / true；加载 false 后 runtime 全局开关为 false，
    ///   最后一次加载 true 又恢复为 true（正负断言各一）。
    #[test]
    fn test_load_path_convention_defaults_on_and_syncs_runtime() {
        let temp = tempfile::TempDir::new().expect("应创建隔离数据目录");
        let store = SettingsStore::new(temp.path().join("path-convention.sqlite"));
        assert!(load_path_convention(&store), "空库默认开启");
        store
            .set_string(PATH_CONVENTION_KEY, "false")
            .expect("写入关闭");
        assert!(!load_path_convention(&store), "保存 false 后应关闭");
        assert!(!runtime::path_convention_enabled(), "关闭应同步到 runtime");
        store
            .set_string(PATH_CONVENTION_KEY, "???")
            .expect("写入乱值");
        assert!(load_path_convention(&store), "未知值按开启处理");
        assert!(
            runtime::path_convention_enabled(),
            "恢复开启应同步到 runtime"
        );
    }
}

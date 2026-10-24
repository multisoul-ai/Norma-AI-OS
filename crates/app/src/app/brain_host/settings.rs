//! 设置页对大脑的读写：配置落盘、重启大脑、安装适配器。

use super::config::{self, Autonomy, BrainSettings};
use crate::app::MultisoulApp;
use brain::BackendKind;
use gpui::Context;
use std::time::Instant;

/// 设置页渲染所需的快照。后端检测结果有缓存，渲染时不做文件 IO。
#[derive(Clone)]
pub(crate) struct BrainSettingsView {
    pub settings: BrainSettings,
    pub running: bool,
    pub installed: Vec<BackendKind>,
    pub installing: bool,
    pub install_error: Option<String>,
}

impl BrainSettingsView {
    /// 实际使用的后端：用户选的；没选时取第一个已安装的。
    pub(crate) fn chosen_backend(&self) -> Option<BackendKind> {
        self.settings
            .backend
            .or_else(|| self.installed.first().copied())
    }
}

const CLAUDE_ADAPTER: &str = "@agentclientprotocol/claude-agent-acp@0.94.0";

impl MultisoulApp {
    /// 大脑正在判断（还没升级给用户）的审批所属的 Agent 卡片。
    pub(crate) fn brain_judging_nodes(&self) -> std::collections::HashSet<u64> {
        self.brain
            .tracked
            .values()
            .filter(|tracked| !tracked.escalated)
            .map(|tracked| tracked.target.node_id)
            .collect()
    }

    /// 标题栏的“语音汇报”开关只在启用了大脑时出现。
    pub(crate) fn brain_toggle_visible(&self) -> bool {
        self.brain
            .settings
            .as_ref()
            .is_some_and(|settings| settings.enabled)
    }

    pub(crate) fn brain_speaking(&self) -> bool {
        self.brain
            .settings
            .as_ref()
            .is_some_and(|settings| settings.enabled && settings.speak)
    }

    /// 只改“是否出声”，不重启大脑：关闭后大脑继续处理审批和派活。
    pub(crate) fn set_brain_speaking(&mut self, speak: bool, cx: &mut Context<Self>) {
        let mut settings = self.brain.settings.clone().unwrap_or_default();
        settings.speak = speak;
        self.brain.settings = Some(settings.clone());
        if !speak {
            self.cancel_agent_announcements();
        }
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = config::save_settings(&settings) {
                    tracing::warn!(%error, "保存大脑配置失败");
                }
            })
            .detach();
        cx.notify();
    }

    pub(crate) fn brain_autonomy(&self) -> Autonomy {
        self.brain.settings.clone().unwrap_or_default().autonomy
    }

    pub(crate) fn brain_settings_view(&self) -> BrainSettingsView {
        BrainSettingsView {
            settings: self.brain.settings.clone().unwrap_or_default(),
            running: self.brain.host.is_some(),
            installed: self.brain.installed.clone(),
            installing: self.brain.installing,
            install_error: self.brain.install_error.clone(),
        }
    }

    /// 打开设置页时调用：读一次配置并刷新已安装的后端（后台线程，不阻塞前台）。
    pub(crate) fn refresh_brain_settings(&mut self, cx: &mut Context<Self>) {
        let work = cx.background_executor().spawn(async move {
            let dir = config::state_dir();
            let installed: Vec<BackendKind> = dir
                .map(|dir| brain::backend::detect(&dir.join("adapters"), &config::search_dirs()))
                .unwrap_or_default()
                .into_iter()
                .map(|backend| backend.kind)
                .collect();
            (config::load_settings(), installed)
        });
        cx.spawn(async move |this, cx| {
            let (settings, installed) = work.await;
            let _ = this.update(cx, |app, cx| {
                app.brain.settings = Some(settings);
                app.brain.installed = installed;
                cx.notify();
            });
        })
        .detach();
    }

    /// 修改并保存设置，然后丢弃正在运行的大脑，下一帧按新配置重新启动。
    pub(crate) fn update_brain_settings(
        &mut self,
        change: impl FnOnce(&mut BrainSettings),
        cx: &mut Context<Self>,
    ) {
        let mut settings = self.brain.settings.clone().unwrap_or_default();
        change(&mut settings);
        self.brain.settings = Some(settings.clone());
        self.brain.host = None;
        self.brain.next_start = Instant::now();
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = config::save_settings(&settings) {
                    tracing::warn!(%error, "保存大脑配置失败");
                }
            })
            .detach();
        cx.notify();
    }

    /// 用户点击后才安装：把 Claude Code 的 ACP 适配器装进 `~/.norma/brain/adapters`。
    pub(crate) fn install_brain_adapter(&mut self, cx: &mut Context<Self>) {
        if self.brain.installing {
            return;
        }
        self.brain.installing = true;
        self.brain.install_error = None;
        let work = cx.background_executor().spawn(async move {
            let dir = config::state_dir()
                .ok_or_else(|| "找不到用户目录".to_string())?
                .join("adapters");
            std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            let path =
                std::env::join_paths(config::search_dirs()).map_err(|error| error.to_string())?;
            let output = std::process::Command::new("npm")
                .args(["install", "--no-audit", "--no-fund", "--prefix"])
                .arg(&dir)
                .arg(CLAUDE_ADAPTER)
                .env("PATH", path)
                .output()
                .map_err(|error| format!("无法运行 npm：{error}"))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .last()
                    .unwrap_or("npm 安装失败")
                    .to_string())
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.brain.installing = false;
                app.brain.install_error = result.err();
                app.refresh_brain_settings(cx);
            });
        })
        .detach();
        cx.notify();
    }
}

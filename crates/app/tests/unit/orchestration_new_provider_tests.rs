use super::controller::app_installer;
use super::status::{delegation_notice, provider_description};
use super::*;
use norma::orchestration::{InstallPaths, OrchestrationInstaller};
use std::collections::HashSet;
use ui::i18n::{text_for, Locale};

const NEW_PROVIDERS: [OrchestrationProvider; 5] = [
    OrchestrationProvider::InfCode,
    OrchestrationProvider::Kimi,
    OrchestrationProvider::ZCode,
    OrchestrationProvider::Pi,
    OrchestrationProvider::Hermes,
];

fn temp_paths() -> InstallPaths {
    InstallPaths::for_home_and_bundle_bin(
        std::path::Path::new("/tmp/norma-test-home"),
        std::path::Path::new("/tmp/norma-test-bundle"),
    )
}

/// 场景：App 路径构造的安装器必须开启 Hermes 插件同步启用。
/// 数据构造：同一份 InstallPaths，分别用 App 构造点与默认 `new` 构造。
/// 过程：读取 hermes_activation_enabled。
/// 预期：正断言 App 安装器为 true；负断言默认安装器为 false（证明开关确实被 App 路径打开，而非默认值）。
#[test]
fn app_installer_enables_hermes_activation() {
    let app = app_installer(temp_paths());
    let plain = OrchestrationInstaller::new(temp_paths(), "1.0.0");

    assert!(
        app.hermes_activation_enabled(),
        "App 安装器必须启用 Hermes 插件同步"
    );
    assert!(
        !plain.hermes_activation_enabled(),
        "默认安装器不得启用，避免测试改动真实配置"
    );
}

/// 场景：四个新 provider 带“委派链路未验证”标注，老 provider 不带；两种语言文案齐全。
/// 数据构造：遍历 ALL；en 与 zh-CN 各取标注 key。
/// 预期：新 provider 为 Some 且非空；Codex/Claude/Cursor/OpenCode/Grok 为 None；
/// 两种语言文案都不等于 key 本身，且互不相同。
#[test]
fn delegation_unverified_notice_only_for_new_providers() {
    for provider in NEW_PROVIDERS {
        let notice = delegation_notice(provider);
        assert!(
            notice.is_some_and(|text| !text.is_empty()),
            "{provider:?} 应带标注"
        );
    }
    for provider in [
        OrchestrationProvider::Codex,
        OrchestrationProvider::Claude,
        OrchestrationProvider::Cursor,
        OrchestrationProvider::OpenCode,
        OrchestrationProvider::Grok,
    ] {
        assert!(
            delegation_notice(provider).is_none(),
            "{provider:?} 不得带标注"
        );
    }
    let key = "settings.delegation_unverified_experimental";
    let en = text_for(Locale::En, key);
    let zh = text_for(Locale::ZhCn, key);
    assert_ne!(en, key, "en 文案缺失");
    assert_ne!(zh, key, "zh-CN 文案缺失");
    assert_ne!(en, zh, "两种语言文案应各自本地化");
}

/// 场景：10 个 provider 的下标与选择器 id 全局唯一，描述覆盖新 provider。
/// 预期：下标 0..10 互异；action 与 disclosure 选择器共 20 个互异；
/// 新 provider 在 NotInstalled 状态有非空描述，且不带 Grok 的“不支持”文案。
#[test]
fn selectors_and_descriptions_cover_all_providers() {
    let all = OrchestrationProvider::ALL;
    let indexes: HashSet<_> = all.iter().map(|p| provider_index(*p)).collect();
    assert_eq!(indexes.len(), 10, "provider_index 必须互异");
    let selectors: HashSet<_> = all
        .iter()
        .flat_map(|p| [action_selector(*p), disclosure_selector(*p)])
        .collect();
    assert_eq!(selectors.len(), 20, "选择器 id 必须全局唯一");
    let grok = provider_description(OrchestrationProvider::Grok, AggregateStatus::NotInstalled);
    for provider in NEW_PROVIDERS {
        let description = provider_description(provider, AggregateStatus::NotInstalled);
        assert!(!description.is_empty(), "{provider:?} 描述不应为空");
        assert_ne!(description, grok, "{provider:?} 不得使用 Grok 的不支持文案");
    }
}

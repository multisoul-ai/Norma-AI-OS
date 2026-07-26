use norma::agent_hook::HookInstallPaths;
use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, ComponentState, InstallPaths, OrchestrationInstaller,
    OrchestrationProvider,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Default)]
struct MemoryAuthorizationRegistry {
    tokens: HashMap<OrchestrationProvider, String>,
}

impl AuthorizationRegistry for MemoryAuthorizationRegistry {
    type Error = String;

    fn authorize(
        &mut self,
        provider: OrchestrationProvider,
        token: &str,
        _installed_version: &str,
    ) -> Result<(), Self::Error> {
        self.tokens.insert(provider, token.to_string());
        Ok(())
    }

    fn revoke(&mut self, provider: OrchestrationProvider) -> Result<(), Self::Error> {
        self.tokens.remove(&provider);
        Ok(())
    }

    fn is_authorized(
        &self,
        provider: OrchestrationProvider,
        token: &str,
    ) -> Result<bool, Self::Error> {
        Ok(self
            .tokens
            .get(&provider)
            .is_some_and(|expected| expected == token))
    }
}

fn install_paths(root: &Path) -> InstallPaths {
    InstallPaths {
        managed_root: root.join("managed"),
        bundled_cli: root.join("bundle").join("norma"),
        bundled_hook: root.join("bundle").join("multisoul-agent-hook"),
        hook_paths: HookInstallPaths {
            claude_settings: root.join(".claude/settings.json"),
            codex_hooks: root.join(".codex/hooks.json"),
            cursor_hooks: root.join(".cursor/hooks.json"),
            grok_hooks: root.join(".grok/hooks.json"),
            opencode_plugin: root.join(".config/opencode/plugins/multisoul-agent-hook.js"),
        },
        claude_skill: root.join(".claude/skills/norma-orchestration/SKILL.md"),
        codex_skill: root.join(".codex/skills/norma-orchestration/SKILL.md"),
        cursor_rule: root.join(".cursor/rules/norma-orchestration.mdc"),
        opencode_skill: root.join(".config/opencode/skills/norma-orchestration/SKILL.md"),
    }
}

/// 旧安装升级：只有委派 Skill 时聚合组件必须 NeedsRepair，修复后补齐展示 Skill。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider = Codex
///   首次完整安装 Skill 文件数 = 2：orchestration + presentation
///   模拟旧版时删除 presentation 1 个，保留 orchestration 1 个
///   required_version = 1.0.0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 完整安装 Codex 四个聚合组件。
///   2. 删除独立 norma-presentation Skill，模拟升级前用户。
///   3. inspect 必须返回 Skill=Missing、aggregate=NeedsRepair。
///   4. install_or_repair 只补齐展示 Skill，并保留原委派 Skill 字节。
///
/// 预期结果：
///   - 正断言：旧安装被识别为 NeedsRepair，修复后恢复 Ready。
///   - 正断言：新展示 Skill 包含 present browser/preview、单轮最多 3 项与项目外确认规则。
///   - 负断言：Repair 不得重写已经 Ready 的委派 Skill，也不得合并成单文件。
#[test]
fn legacy_install_without_presentation_skill_needs_repair_and_is_upgraded_in_place() {
    let temp = tempfile::tempdir().expect("测试应创建隔离升级目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"codex-upgrade-cli").expect("测试应写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"codex-upgrade-hook").expect("测试应写入 bundled Hook");
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("测试前置条件应完整安装 Codex");
    let presentation_path = paths
        .presentation_skill_path(OrchestrationProvider::Codex)
        .expect("Codex 应有 presentation Skill 路径");
    let original_orchestration =
        fs::read(&paths.codex_skill).expect("委派 Skill 应在完整安装后存在");
    fs::remove_file(&presentation_path).expect("测试应删除展示 Skill 模拟旧安装");

    let legacy = installer.inspect(OrchestrationProvider::Codex, &registry);
    let repaired = installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Repair 应补齐缺失展示 Skill");
    let presentation = fs::read_to_string(&presentation_path).expect("Repair 后展示 Skill 应存在");

    assert_eq!(
        legacy.skill_component,
        ComponentState::Missing,
        "两个受管 Skill 缺少任意一个时聚合 Skill 组件必须为 Missing"
    );
    assert_eq!(
        legacy.aggregate_status,
        AggregateStatus::NeedsRepair,
        "旧安装其它组件 Ready、展示 Skill 缺失时必须显示 NeedsRepair"
    );
    assert_eq!(
        repaired.aggregate_status,
        AggregateStatus::Ready,
        "Repair 补齐两个 Skill 后 Provider 必须恢复 Ready"
    );
    assert!(
        presentation.contains("norma present browser")
            && presentation.contains("norma present preview"),
        "展示 Skill 必须明确提供两个独立 present 命令"
    );
    assert!(
        presentation.contains("at most 3") || presentation.contains("最多 3"),
        "展示 Skill 必须写明单轮最多展示三项"
    );
    assert!(
        presentation.contains("explicitly supplied by the user")
            && presentation.contains("outside the current project")
            && presentation.contains("confirm"),
        "展示 Skill 必须允许用户显式路径直接展示，并要求 Agent 主动选择项目外文件时先确认"
    );
    assert_eq!(
        fs::read(&paths.codex_skill).expect("Repair 后委派 Skill 仍应存在"),
        original_orchestration,
        "Repair 不得重写已经是当前版本的委派 Skill"
    );
    assert_ne!(
        paths.codex_skill, presentation_path,
        "委派与展示能力必须保持两个独立 Skill 文件"
    );
}

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

/// 已安装版本升级：旧版受管 presentation Skill 必须因版本不同而重写，得到 Ego 交接规则。
///
/// 数据构造（含关键数值的推导过程）：
///   installed_version = "0.1.47"（Ego 功能前的受管 Skill 版本）
///   required_version = env!("CARGO_PKG_VERSION") = "0.1.48"
///   managed Skill files = 2：orchestration + presentation，二者均标记为旧版本
///   需要新增的 Ego 生命周期命令 = present(1) + running(1) + handoff(1) + complete(1) = 4
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入两个带 Norma marker 的 0.1.47 受管 Skill，模拟用户已安装旧 App。
///   2. 用当前 App 版本检查 Provider，读取 Outdated 状态及版本差异。
///   3. 运行 repair，读取重写后的 presentation Skill。
///
/// 预期结果：
///   - 正断言：旧 Skill 被判定为 Outdated，repair 后 Provider 恢复 Ready。
///   - 正断言：重写内容带当前版本，并包含 Ego 创建、交接与禁止自动 takeover 的规则。
///   - 负断言：重写后的 Skill 不得继续携带 0.1.47 marker。
#[test]
fn prior_version_presentation_skill_is_repaired_with_ego_rules() {
    let temp = tempfile::tempdir().expect("测试应创建隔离的旧版本安装目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"norma-current-cli").expect("测试应写入当前 bundled CLI");
    fs::write(&paths.bundled_hook, b"norma-current-hook").expect("测试应写入当前 bundled Hook");
    fs::create_dir_all(
        paths
            .codex_skill
            .parent()
            .expect("Codex orchestration Skill 应有父目录"),
    )
    .expect("测试应创建 Codex orchestration Skill 目录");
    let presentation_path = paths
        .presentation_skill_path(OrchestrationProvider::Codex)
        .expect("Codex 应有 presentation Skill 路径");
    fs::create_dir_all(
        presentation_path
            .parent()
            .expect("Codex presentation Skill 应有父目录"),
    )
    .expect("测试应创建 Codex presentation Skill 目录");
    fs::write(
        &paths.codex_skill,
        "# norma-orchestration-managed\n# managed-version: 0.1.47\nold orchestration\n",
    )
    .expect("测试应写入旧版 orchestration Skill");
    fs::write(
        &presentation_path,
        "# norma-presentation-managed\n# managed-version: 0.1.47\nold presentation\n",
    )
    .expect("测试应写入旧版 presentation Skill");

    let installer = OrchestrationInstaller::new(paths.clone(), env!("CARGO_PKG_VERSION"));
    let mut registry = MemoryAuthorizationRegistry::default();
    let before = installer.inspect(OrchestrationProvider::Codex, &registry);
    let repaired = installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("当前 App 版本应能 repair 旧版受管 Skill");
    let presentation =
        fs::read_to_string(&presentation_path).expect("repair 后 presentation Skill 应可读取");

    assert_eq!(
        before.skill_component,
        ComponentState::Outdated {
            installed_version: "0.1.47".to_string(),
            required_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        "旧版受管 Skill 必须被识别为 Outdated，而非错误视为 Ready"
    );
    assert_eq!(
        repaired.aggregate_status,
        AggregateStatus::Ready,
        "repair 后当前版本的 Codex Provider 必须恢复 Ready"
    );
    assert!(
        presentation.contains(&format!("# managed-version: {}", env!("CARGO_PKG_VERSION"))),
        "repair 后 presentation Skill 必须写入当前 App 版本 marker"
    );
    assert!(
        presentation.contains("norma present ego")
            && presentation.contains("norma ego handoff")
            && presentation.contains("Do not automatically call `takeOverTaskSpace`"),
        "当前版本 repair 后的 presentation Skill 必须包含 Ego 创建、交接和禁止自动接管规则"
    );
    assert!(
        !presentation.contains("# managed-version: 0.1.47"),
        "repair 后 presentation Skill 不得继续携带旧版本 marker"
    );
}

/// 同版本受管 Skill 内容修复：旧的 Ego 完成规则必须被识别并重写为保留 Task Space 的规则。
///
/// 数据构造（含关键数值的推导过程）：
///   required_version = "1.0.0"
///   managed Skill files = 2：orchestration(当前内容) + presentation(同版本旧内容)
///   presentation marker version = required_version = "1.0.0"
///   Ego 完成保留规则 = `keep: true` 1 条；危险关闭规则 = `keep: false` 0 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 完整安装 1.0.0 的 Codex 聚合组件，确保 CLI、Hook、授权和两个 Skill 均 Ready。
///   2. 只把 presentation Skill 替换为仍标记 1.0.0、但缺少页面保留规则的旧内容。
///   3. inspect 比较受管文件的实际内容与当前模板，必须把同版本旧内容判为 Outdated。
///   4. install_or_repair 重写 presentation，再读取最终文件。
///
/// 预期结果：
///   - 正断言：同版本旧内容被判为 Outdated，修复后聚合状态恢复 Ready。
///   - 正断言：修复后的完成流程显式调用 `completeTaskSpace(..., { keep: true })`。
///   - 负断言：完成流程不得要求 `keep: false`，否则任务结束会让画布失去最终页面。
#[test]
fn same_version_stale_presentation_is_repaired_with_ego_page_retention() {
    let temp = tempfile::tempdir().expect("测试应创建隔离的同版本 Skill 安装目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"norma-current-cli").expect("测试应写入当前 bundled CLI");
    fs::write(&paths.bundled_hook, b"norma-current-hook").expect("测试应写入当前 bundled Hook");
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("测试前置条件应完整安装当前版本 Codex 组件");
    let presentation_path = paths
        .presentation_skill_path(OrchestrationProvider::Codex)
        .expect("Codex 应有 presentation Skill 路径");
    fs::write(
        &presentation_path,
        "---\nname: norma-presentation\ndescription: stale\n---\n\
         # norma-presentation-managed\n# managed-version: 1.0.0\n\
         Run `norma ego complete <node_id>` when finished.\n",
    )
    .expect("测试应写入同版本但内容过期的 presentation Skill");

    let stale = installer.inspect(OrchestrationProvider::Codex, &registry);
    let repaired = installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Repair 应重写同版本但内容过期的受管 Skill");
    let presentation =
        fs::read_to_string(&presentation_path).expect("修复后的 presentation Skill 应可读取");

    assert_eq!(
        stale.skill_component,
        ComponentState::Outdated {
            installed_version: "1.0.0".to_string(),
            required_version: "1.0.0".to_string(),
        },
        "同版本 marker 不能掩盖旧模板内容，presentation Skill 必须判为 Outdated"
    );
    assert_eq!(
        repaired.aggregate_status,
        AggregateStatus::Ready,
        "重写同版本旧 Skill 后 Codex 聚合组件必须恢复 Ready"
    );
    assert!(
        presentation.contains("completeTaskSpace(task_space_id, { keep: true })"),
        "Ego 完成规则必须保留 Task Space，画布才能继续显示最终页面"
    );
    assert!(
        !presentation.contains("completeTaskSpace(task_space_id, { keep: false })"),
        "Ego 卡片仍关联任务时不得关闭 Task Space，否则画布会丢失实时与最终内容"
    );
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
            kimi_config: (root.join(".config/opencode/plugins/multisoul-agent-hook.js"))
                .with_file_name("kimi-config.toml"),
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
///   展示命令类别 = Browser(1) + Preview(1) + Ego(1) = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 完整安装 Codex 四个聚合组件。
///   2. 删除独立 norma-presentation Skill，模拟升级前用户。
///   3. inspect 必须返回 Skill=Missing、aggregate=NeedsRepair。
///   4. install_or_repair 只补齐展示 Skill，并保留原委派 Skill 字节。
///   5. 读取修复后的说明，验证 Ego 的创建、运行、交接和显式恢复约束。
///
/// 预期结果：
///   - 正断言：旧安装被识别为 NeedsRepair，修复后恢复 Ready。
///   - 正断言：新展示 Skill 包含 present browser/preview/ego、单轮最多 3 项与项目外确认规则。
///   - 正断言：Ego 指令涵盖 running/handoff/complete，且明确禁止用户交还前自动 takeOver。
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
        presentation.contains("norma present ego")
            && presentation.contains("norma ego running")
            && presentation.contains("norma ego handoff")
            && presentation.contains("norma ego complete"),
        "展示 Skill 必须明确提供 Ego 的创建、工作、交接和完成命令"
    );
    assert!(
        presentation.contains("Do not automatically call `takeOverTaskSpace`")
            && presentation.contains("only after the user clicks Return to AI"),
        "展示 Skill 必须阻止 Agent 在用户交还前自动夺回 Ego 控制权"
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

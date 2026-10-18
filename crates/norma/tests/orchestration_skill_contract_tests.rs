use norma::agent_hook::HookInstallPaths;
use norma::orchestration::{
    AuthorizationRegistry, InstallPaths, OrchestrationInstaller, OrchestrationProvider,
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
            claude_settings: root.join(".claude").join("settings.json"),
            codex_hooks: root.join(".codex").join("hooks.json"),
            cursor_hooks: root.join(".cursor").join("hooks.json"),
            grok_hooks: root.join(".grok").join("hooks.json"),
            pi_extension: root.join("norma-agent-hook.ts"),
            opencode_plugin: root
                .join(".config")
                .join("opencode")
                .join("plugins")
                .join("multisoul-agent-hook.js"),
        },
        claude_skill: root
            .join(".claude")
            .join("skills")
            .join("norma-orchestration")
            .join("SKILL.md"),
        codex_skill: root
            .join(".codex")
            .join("skills")
            .join("norma-orchestration")
            .join("SKILL.md"),
        cursor_rule: root
            .join(".cursor")
            .join("rules")
            .join("norma-orchestration.mdc"),
        opencode_skill: root
            .join(".config")
            .join("opencode")
            .join("skills")
            .join("norma-orchestration")
            .join("SKILL.md"),
    }
}

/// 编排 Skill 契约：跨 Workspace 委派必须显式选目标，并安全处理重试与 untracked。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider 数 = 1（Codex）
///   Workspace 发现命令数 = 1（workspace list）
///   目标选择参数数 = 1（--workspace <workspace_id>）
///   幂等键数 = 1（同一 operation_id 在不确定结果后继续复用）
///   结果读取路径 = wait 终态直返(1) + result 单次查询(1) = 2 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离目录安装 Codex 的受管编排 Skill。
///   2. 读取安装后的 SKILL.md，检查跨 Workspace 发现与显式路由说明。
///   3. 检查诊断、幂等重试、终态结果读取和 untracked 安全边界。
///
/// 预期结果：
///   - 正断言：Skill 要求使用 workspace list、--workspace、doctor、version 和 operation_id。
///   - 正断言：不确定结果只能复用原 operation_id，终态结果可以直接完成。
///   - 正断言：wait 终态直接消费 result，也可用 task result 精确查询。
///   - 负断言：不得建议每窗口启动服务，也不得自动使用 --force-untracked。
#[test]
fn managed_orchestration_skill_documents_global_cross_workspace_contract() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"codex-test-cli").expect("测试应写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"codex-test-hook").expect("测试应写入 bundled Hook");
    let installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();

    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Codex 编排 Skill 安装应成功");
    let source = fs::read_to_string(&paths.codex_skill).expect("安装后的 Codex Skill 应可读取");

    assert!(
        source.contains("workspace list"),
        "跨 Workspace 委派前必须通过 workspace list 发现稳定 Workspace ID"
    );
    assert!(
        source.contains("--workspace <workspace_id>"),
        "跨 Workspace 的 agent list 与 task assign 必须显式携带目标 Workspace ID"
    );
    assert!(
        source.contains("--format json"),
        "受管 Skill 必须只消费稳定 JSON，不得解析自然语言输出"
    );
    assert!(
        source.contains("doctor") && source.contains("--version"),
        "app_unavailable 或协议问题必须用 doctor 与 version 提供真实诊断证据"
    );
    assert!(
        source.contains("--operation-id <operation_id>"),
        "有副作用的 assign 必须显式携带可复用的 operation_id"
    );
    assert!(
        source.contains("reuse the same operation_id"),
        "结果不确定时必须复用原 operation_id，避免重复创建任务"
    );
    assert!(
        source.contains("task cancel <task_id> --operation-id <operation_id>"),
        "取消任务也必须显式保留 operation ID，才能安全处理 outcome_unknown"
    );
    assert!(
        source.contains("terminal status"),
        "已存在终态结果时 Skill 必须允许直接按终态完成"
    );
    assert!(
        source.contains("wait <task_id> --format json")
            && source.contains("terminal response includes the task result"),
        "受管 Skill 必须明确 wait 的终态响应直接携带任务结果"
    );
    assert!(
        source.contains("task result <task_id> --format json")
            && source.contains("task_result_not_ready"),
        "受管 Skill 必须说明独立 result 查询及其可重试未就绪错误"
    );
    assert!(
        source.contains("managed receipt") && source.contains("untracked receipt"),
        "Skill 必须只对 managed 回执自动 wait，并在 untracked 回执后停止"
    );
    assert!(
        source.contains("bounded backoff") && source.contains("orchestration_checking"),
        "健康检查中只能做有界退避的只读重试"
    );
    assert!(
        source.contains("workspace_not_open")
            && source.contains("ambiguous_workspace")
            && source.contains("protocol_version_mismatch"),
        "确定性路由或版本错误必须停止，不能自动开窗或修复"
    );
    assert!(
        source.contains("Do not start one NormaServer per window"),
        "Skill 必须明确 NormaServer 是 App 全局服务，不应按窗口补启动"
    );
    assert!(
        source.contains("Do not use `--force-untracked` unless the user explicitly"),
        "Skill 必须保留 force-untracked 的显式用户授权边界"
    );
    assert!(
        !source.contains("retry with a new operation_id"),
        "Skill 不得建议用新 operation_id 盲目重试同一副作用"
    );
    assert!(
        !source.contains("automatically use `--force-untracked`"),
        "Skill 不得建议自动绕过 untracked 的可信终态限制"
    );
}

/// 编排 Skill 发现契约：中文口语化与链式子委派请求必须在读取正文前命中描述。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider 数 = 1（Codex）
///   中文直接委派示例 = 1（给 Simon 发布一个任务）
///   中文链式委派示例 = 1（让 Simon 给浩辰发布一个任务）
///   语音转写噪音类型 = 2（多余空格 + 标点）
///   负向边界 = 2（仅提及 Agent + 自主委派）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离目录安装 Codex 的受管编排 Skill。
///   2. 只读取 YAML frontmatter 的 description，模拟 Provider 加载正文前的发现阶段。
///   3. 检查描述能召回中文直接委派、链式子委派和带空格/标点的语音转写。
///   4. 检查正文只派发首个目标，并把下一层委派完整保留在任务正文中。
///
/// 预期结果：
///   - 正断言：description 明确要求在 assign/delegate/publish 与中文发布任务表达下使用 Skill。
///   - 正断言：description 覆盖 Simon → 浩辰的链式子委派和语音转写噪音。
///   - 正断言：正文要求调用方只派发 Simon，并保留 Simon → 浩辰的下一层指令。
///   - 负断言：description 不把普通名称提及或未经请求的自主委派纳入触发范围。
///   - 负断言：正文不得要求调用方越过 Simon 直接派发全部具名目标。
#[test]
fn managed_orchestration_skill_description_recalls_chinese_nested_delegation() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"codex-test-cli").expect("测试应写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"codex-test-hook").expect("测试应写入 bundled Hook");
    let installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();

    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Codex 编排 Skill 安装应成功");
    let source = fs::read_to_string(&paths.codex_skill).expect("安装后的 Codex Skill 应可读取");
    let description = source
        .lines()
        .find_map(|line| line.strip_prefix("description: "))
        .expect("Codex Skill frontmatter 必须包含 description");

    assert!(
        description.contains("assign/delegate/publish")
            && description.contains("给 Simon 发布一个任务"),
        "发现描述必须同时覆盖通用委派动作与中文‘发布任务’表达，实际为：{description}"
    );
    assert!(
        description.contains("让 Simon 给浩辰发布一个任务")
            && description.contains("nested/subtask delegation"),
        "发现描述必须覆盖 Simon → 浩辰链式子委派，实际为：{description}"
    );
    assert!(
        description.contains("extra spaces or punctuation"),
        "发现描述必须提醒模型容忍语音转写中的多余空格和标点，实际为：{description}"
    );
    assert!(
        description.contains("Do not use for mere mentions or unsolicited delegation"),
        "提高召回时仍必须保留普通提及和自主委派的负向边界，实际为：{description}"
    );
    assert!(
        source.contains("Assign only the first target yourself")
            && source.contains("preserve the nested")
            && source.contains("delegation instruction in that task text"),
        "链式委派正文必须只派发首个目标并保留下一层委派指令"
    );
    assert!(
        !description.contains("only when the user says delegate"),
        "发现描述不得把触发条件收窄为必须出现英文 delegate，实际为：{description}"
    );
    assert!(
        !source.contains("Assign every named target yourself"),
        "链式委派不得越过首个目标，由调用方直接派发全部具名 Agent"
    );
}

/// 编排 Skill 契约：确定性拒绝码（self_dispatch_forbidden / delegation_cycle /
/// delegation_budget_exceeded）必须给出停止动作，并明确禁止换 operation_id 重试。
///
/// 数据构造（含关键数值的推导过程）：
///   确定性拒绝码数 = 3（self_dispatch_forbidden、delegation_cycle、delegation_budget_exceeded）
///   禁止动作数     = 1（不得生成新 operation_id）
///   停止语义       = 2（立即停止 + 不换目标拼写/Workspace）
///   Provider 数    = 1（Codex）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离目录安装 Codex 的受管编排 Skill。
///   2. 读取 SKILL.md 源文本。
///   3. 检查三个确定性拒绝码、停止动作与新 operation_id 禁令。
///
/// 预期结果：
///   - 正断言：源文本包含 self_dispatch_forbidden 的自身身份解释与停止动作。
///   - 正断言：源文本包含 delegation_cycle、delegation_budget_exceeded 的停止规则。
///   - 正断言：源文本明确禁止为这三个确定性错误换新 operation_id。
///   - 负断言：不得建议对确定性错误盲目重试或换目标拼写/Workspace。
#[test]
fn managed_orchestration_skill_stops_on_deterministic_rejection_codes() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"codex-test-cli").expect("测试应写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"codex-test-hook").expect("测试应写入 bundled Hook");
    let installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();

    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Codex 编排 Skill 安装应成功");
    let source = fs::read_to_string(&paths.codex_skill).expect("安装后的 Codex Skill 应可读取");

    assert!(
        source.contains("self_dispatch_forbidden"),
        "Skill 必须把 self_dispatch_forbidden 归类为确定性拒绝码，实际缺失"
    );
    assert!(
        source.contains("delegation_cycle"),
        "Skill 必须把 delegation_cycle 归类为确定性拒绝码，实际缺失"
    );
    assert!(
        source.contains("delegation_budget_exceeded"),
        "Skill 必须把 delegation_budget_exceeded 归类为确定性拒绝码，实际缺失"
    );
    assert!(
        source.contains("stop") && source.contains("self_dispatch_forbidden"),
        "self_dispatch_forbidden 必须明确要求立即停止，防止换参数重试"
    );
    assert!(
        source.contains("do not") && source.contains("new operation_id"),
        "确定性拒绝码必须明确禁止生成新的 operation_id，防止重复派发循环"
    );
    assert!(
        !source.contains("retry with a new operation_id"),
        "Skill 不得以任何形式建议用新 operation_id 重试确定性拒绝"
    );
}

/// 编排 Skill 契约：同 Workspace 委派前必须先用 `agent self` 识别自我身份，跨 Workspace 同名不算自己。
///
/// 数据构造（含关键数值的推导过程）：
///   决策规则数 = 5（先 self 再 list、命中自我不 assign、跨 Workspace 同名非 self、
///   self 查询失败停止、Workspace 作用域声明）
///   Provider 数 = 1（Codex）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离目录安装 Codex 的受管编排 Skill。
///   2. 读取 SKILL.md 源文本。
///   3. 检查自我身份自查规则与 Workspace 作用域边界。
///
/// 预期结果：
///   - 正断言：Skill 要求同 Workspace 委派前运行 `agent self --format json`。
///   - 正断言：命中自我时不得生成 operation_id、不得调用 assign。
///   - 正断言：跨 Workspace 同名明确不算自己；self 查询失败必须停止派发。
///   - 负断言：不得把跨 Workspace 同名 Agent 当作自我身份。
#[test]
fn managed_orchestration_skill_documents_self_identity_probe_before_assign() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"codex-test-cli").expect("测试应写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"codex-test-hook").expect("测试应写入 bundled Hook");
    let installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();

    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Codex 编排 Skill 安装应成功");
    let source = fs::read_to_string(&paths.codex_skill).expect("安装后的 Codex Skill 应可读取");

    assert!(
        source.contains("agent self --format json"),
        "Skill 必须提供 agent self 自查命令，实际缺失"
    );
    assert!(
        source.contains("Before any same-Workspace named delegation"),
        "每个同 Workspace 委派前必须先查询自我身份"
    );
    assert!(
        source.contains("do not generate an operation_id and do not call task assign"),
        "命中自我时必须不生成 operation_id、不调用 task assign"
    );
    assert!(
        source.contains("The same name in a different Workspace is not yourself"),
        "跨 Workspace 同名必须明确不等于自我身份"
    );
    assert!(
        source.contains("Workspace-scoped"),
        "自我身份必须声明 Workspace 作用域边界"
    );
    assert!(
        source.contains("stop dispatching") && source.contains("cannot be confirmed"),
        "self 查询失败时必须停止派发并报告无法确认身份"
    );
}

//! Agent hook launch environment for terminal-backed Agent nodes.

use crate::runtime::Runtime;
use canvas::{AgentProfile, NodeId};
use norma::agent_hook::{
    ENV_AGENT_PROFILE, ENV_CONTROL_RUN_ID, ENV_HOOK_ENDPOINT, ENV_HOOK_TOKEN, ENV_NODE_ID,
    ENV_PATH_CONVENTION,
};
use std::sync::atomic::{AtomicBool, Ordering};
use terminal::PtyCommand;

/// 父进程若是另一张 Agent 卡片，这些变量会把本进程和它的子进程串成那张卡。
/// `NORMA_CONTROL_RUN_ID` 由 `agent_hooks::question::attach` 注入。
pub const INHERITED_AGENT_IDENTITY_ENV: &[&str] = &[
    ENV_NODE_ID,
    ENV_AGENT_PROFILE,
    ENV_HOOK_ENDPOINT,
    ENV_HOOK_TOKEN,
    "NORMA_WORKSPACE_ID",
    "NORMA_WORKSPACE_ROUTE_ID",
    "NORMA_SOCKET",
    ENV_PATH_CONVENTION,
    ENV_CONTROL_RUN_ID,
];

/// 清掉从父进程继承的 Norma 卡片身份。必须在任何其他线程启动前调用。
pub fn clear_inherited_agent_identity() {
    for key in INHERITED_AGENT_IDENTITY_ENV {
        std::env::remove_var(key);
    }
}

pub(crate) fn without_inherited_agent_identity(mut command: PtyCommand) -> PtyCommand {
    for key in INHERITED_AGENT_IDENTITY_ENV {
        command = command.env_remove(*key);
    }
    command
}
mod question;

/// 用户在“编排”设置里是否允许向 Agent 注入文件路径输出约定；默认开启。
static PATH_CONVENTION_ENABLED: AtomicBool = AtomicBool::new(true);

/// 只影响之后启动的 Agent：开关经启动环境变量 `NORMA_PATH_CONVENTION=0` 传给 hook 与插件。
pub fn set_path_convention_enabled(enabled: bool) {
    PATH_CONVENTION_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn path_convention_enabled() -> bool {
    PATH_CONVENTION_ENABLED.load(Ordering::Relaxed)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookRuntimeConfig {
    pub endpoint: String,
    pub token: String,
    pub workspace_id: String,
    pub workspace_route_id: String,
    pub norma_socket: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentHookEnv {
    pub(crate) control_run_id: Option<String>,
    pub(crate) node_id: NodeId,
    pub(crate) profile: AgentProfile,
    pub(crate) endpoint: String,
    pub(crate) token: String,
    pub(crate) workspace_id: String,
    pub(crate) workspace_route_id: String,
    pub(crate) norma_socket: String,
}

impl AgentHookEnv {
    pub(crate) fn apply_to_command(&self, command: PtyCommand) -> PtyCommand {
        self.apply_to_command_with(command, path_convention_enabled())
    }

    /// 显式传入路径约定开关，便于测试且不依赖全局状态；总是写入 `0`/`1`，
    /// 这样用户 shell 里残留的同名变量不会覆盖 Norma 设置。
    pub(crate) fn apply_to_command_with(
        &self,
        command: PtyCommand,
        path_convention: bool,
    ) -> PtyCommand {
        question::attach(command, self)
            .env(
                "NORMA_PATH_CONVENTION",
                if path_convention { "1" } else { "0" },
            )
            .env("NORMA_NODE_ID", self.node_id.to_string())
            .env("NORMA_AGENT_PROFILE", agent_profile_env(self.profile))
            .env("NORMA_HOOK_ENDPOINT", self.endpoint.clone())
            .env("NORMA_HOOK_TOKEN", self.token.clone())
            .env("NORMA_WORKSPACE_ID", self.workspace_id.clone())
            .env("NORMA_WORKSPACE_ROUTE_ID", self.workspace_route_id.clone())
            .env("NORMA_SOCKET", self.norma_socket.clone())
    }
}

impl Runtime {
    pub fn set_agent_hook_config(&mut self, config: Option<AgentHookRuntimeConfig>) {
        self.agent_hook_config = config;
    }

    pub fn agent_hook_config(&self) -> Option<&AgentHookRuntimeConfig> {
        self.agent_hook_config.as_ref()
    }

    pub(crate) fn agent_hook_env(
        &self,
        node_id: NodeId,
        profile: AgentProfile,
    ) -> Option<AgentHookEnv> {
        self.agent_hook_config.as_ref().map(|config| AgentHookEnv {
            control_run_id: self.native_question_process_id(node_id).map(str::to_owned),
            node_id,
            profile,
            endpoint: config.endpoint.clone(),
            token: config.token.clone(),
            workspace_id: config.workspace_id.clone(),
            workspace_route_id: config.workspace_route_id.clone(),
            norma_socket: config.norma_socket.clone(),
        })
    }
}

fn agent_profile_env(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::Cursor => "cursor",
        AgentProfile::Grok => "grok",
        AgentProfile::OpenCode => "opencode",
        AgentProfile::InfCode => "infcode",
        AgentProfile::Kimi => "kimi",
        AgentProfile::ZCode => "zcode",
        AgentProfile::Pi => "pi",
        AgentProfile::Hermes => "hermes",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OpenCode Agent nodes should receive the same hook bridge environment as Claude/Codex.
    ///
    /// Data setup:
    ///   node_id = 77, representing one OpenCode Agent card.
    ///   profile = OpenCode, so NORMA_AGENT_PROFILE should be "opencode".
    ///   endpoint/token come from AgentHookRuntimeConfig started by the app-local hook server.
    ///
    /// Execution:
    ///   1. Create Runtime and set AgentHookRuntimeConfig.
    ///   2. Request agent_hook_env(node_id, OpenCode).
    ///   3. Inspect the returned AgentHookEnv fields.
    ///
    /// Expected:
    ///   - Positive: env exists for OpenCode so the OpenCode plugin can POST status events.
    ///   - Positive: profile remains OpenCode and endpoint/token are preserved.
    ///   - Negative: OpenCode must not be treated as Codex.
    #[test]
    fn test_agent_hook_env_includes_opencode_profile() {
        let mut runtime = Runtime::new();
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:49154/agent-hooks".to_string(),
            token: "tok_opencode".to_string(),
            workspace_id: "workspace-alpha".to_string(),
            workspace_route_id: "route-alpha-7".to_string(),
            norma_socket: "/tmp/multisoul-norma-dev.sock".to_string(),
        }));

        let env = runtime
            .agent_hook_env(77, AgentProfile::OpenCode)
            .expect("OpenCode Agent should receive hook env");

        assert_eq!(
            env.node_id, 77,
            "OpenCode hook env should preserve the target node id"
        );
        assert_eq!(
            env.profile,
            AgentProfile::OpenCode,
            "OpenCode hook env should preserve profile=OpenCode"
        );
        assert_eq!(
            env.endpoint, "http://127.0.0.1:49154/agent-hooks",
            "OpenCode hook env should preserve the app-local endpoint"
        );
        assert_eq!(
            env.token, "tok_opencode",
            "OpenCode hook env should preserve the hook auth token"
        );
        assert_eq!(
            env.workspace_id, "workspace-alpha",
            "Agent env 必须保留稳定 Workspace ID，供 Norma CLI 解析来源"
        );
        assert_eq!(
            env.workspace_route_id, "route-alpha-7",
            "Agent env 必须保留运行期 route ID，防止旧 Session 命中新窗口"
        );
        assert_ne!(
            env.profile,
            AgentProfile::Codex,
            "OpenCode hook env must not be normalized to Codex"
        );
    }

    /// 用户在“编排”设置里关闭路径约定后，之后启动的 Agent 带 `NORMA_PATH_CONVENTION=0`；默认不带。
    ///
    /// 数据构造：Codex/42 + 任意 hook 配置。
    /// 执行过程：用显式开关参数各应用一次（不触碰全局状态，避免同进程测试互相干扰）。
    /// 预期结果：开启时命令含 `NORMA_PATH_CONVENTION=1`，关闭时含 `=0`，且 hook 环境（NORMA_NODE_ID）都在。
    #[test]
    fn path_convention_switch_reaches_agent_environment() {
        let mut runtime = Runtime::new();
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:49152/agent-hooks".to_string(),
            token: "tok".to_string(),
            workspace_id: "w".to_string(),
            workspace_route_id: "r".to_string(),
            norma_socket: "/tmp/s.sock".to_string(),
        }));
        let env = runtime.agent_hook_env(42, AgentProfile::Codex).unwrap();
        let has = |command: &PtyCommand, key: &str, value: &str| {
            command.env.iter().any(|(k, v)| k == key && v == value)
        };
        let on = env.apply_to_command_with(PtyCommand::new("/bin/zsh"), true);
        assert!(has(&on, "NORMA_NODE_ID", "42"), "hook 环境仍应注入");
        assert!(
            has(&on, "NORMA_PATH_CONVENTION", "1"),
            "开启时显式传 1，覆盖 shell 残留值"
        );
        let off = env.apply_to_command_with(PtyCommand::new("/bin/zsh"), false);
        assert!(has(&off, "NORMA_PATH_CONVENTION", "0"), "关闭后应显式传 0");
    }

    /// Agent 启动上下文必须显式绑定所属 Norma socket，不能依赖全局安装 CLI 的构建类型。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   Agent node = Codex/42(1)
    ///   owning socket = /tmp/multisoul-norma-dev.sock(1)
    ///   foreign socket = /tmp/multisoul-norma.sock(1)
    ///   workspace identity = workspace-alpha + route-alpha-7(2 fields)
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 为开发版 Workspace 构造包含所属 socket 的 AgentHookRuntimeConfig。
    ///   2. 从 Runtime 解析 Codex AgentHookEnv。
    ///   3. 将该环境应用到实际 PtyCommand，并检查最终进程环境。
    ///
    /// 预期结果：
    ///   - 正断言：命令精确包含 `NORMA_SOCKET=/tmp/multisoul-norma-dev.sock`。
    ///   - 正断言：Workspace ID 与 route ID 仍同时注入。
    ///   - 负断言：命令不得回退到正式版 `/tmp/multisoul-norma.sock`。
    #[test]
    fn agent_command_is_pinned_to_the_owning_norma_socket() {
        let mut runtime = Runtime::new();
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:49152/agent-hooks".to_string(),
            token: "tok_codex".to_string(),
            workspace_id: "workspace-alpha".to_string(),
            workspace_route_id: "route-alpha-7".to_string(),
            norma_socket: "/tmp/multisoul-norma-dev.sock".to_string(),
        }));

        let env = runtime
            .agent_hook_env(42, AgentProfile::Codex)
            .expect("已注册 Workspace 的 Codex Agent 必须获得编排环境");
        let command = env.apply_to_command(PtyCommand::new("/bin/zsh"));

        assert!(
            command.env.iter().any(|(key, value)| {
                key == "NORMA_SOCKET" && value == "/tmp/multisoul-norma-dev.sock"
            }),
            "Agent 命令必须显式绑定开发版所属 socket，实际环境：{:?}",
            command.env
        );
        assert!(
            command
                .env
                .iter()
                .any(|(key, value)| key == "NORMA_WORKSPACE_ID" && value == "workspace-alpha"),
            "显式 socket 绑定不得丢失 Workspace ID，实际环境：{:?}",
            command.env
        );
        assert!(
            command.env.iter().any(|(key, value)| {
                key == "NORMA_WORKSPACE_ROUTE_ID" && value == "route-alpha-7"
            }),
            "显式 socket 绑定不得丢失 Workspace route ID，实际环境：{:?}",
            command.env
        );
        assert!(
            command.env.iter().all(|(key, value)| {
                key != "NORMA_SOCKET" || value != "/tmp/multisoul-norma.sock"
            }),
            "开发版 Agent 不得回退到正式版 Norma socket"
        );
    }

    /// 启动时清掉继承身份后，普通子进程也不得再看到来源卡片的变量。
    /// macOS 的 `ps eww` 仍显示 exec 时的环境，不能用来判断 `remove_var`。
    ///
    /// 数据构造：
    ///   `INHERITED_AGENT_IDENTITY_ENV` 的每个键都写成哨兵 `900001`（9 个键）。
    ///   探测命令是 `/bin/sh`，按同一顺序打印 `${KEY-unset}`。
    ///
    /// 执行过程：
    ///   1. 写入哨兵，先 spawn 一次，确认子进程看得到 `900001`。
    ///   2. 调用 `clear_inherited_agent_identity`。
    ///   3. 再 spawn 一次，读取每个键。
    ///   4. Drop 恢复测试前的环境。
    ///
    /// 预期结果：
    ///   - 正断言：清除后 9 个键在子进程里都是 `unset`。
    ///   - 负断言：清除前 `NORMA_NODE_ID` 必须是 `900001`，否则没有证明清除生效。
    #[test]
    fn clear_inherited_agent_identity_hides_vars_from_child_processes() {
        struct Restore(Vec<(String, Option<String>)>);
        impl Drop for Restore {
            fn drop(&mut self) {
                for (key, previous) in &self.0 {
                    match previous {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
        let _restore = Restore(
            INHERITED_AGENT_IDENTITY_ENV
                .iter()
                .map(|key| ((*key).to_string(), std::env::var(key).ok()))
                .collect(),
        );
        for key in INHERITED_AGENT_IDENTITY_ENV {
            std::env::set_var(key, "900001");
        }

        let before = child_identity_env();
        clear_inherited_agent_identity();
        let after = child_identity_env();

        assert_eq!(
            before.first().map(String::as_str),
            Some("900001"),
            "清除前子进程必须看到 NORMA_NODE_ID=900001，实际：{before:?}"
        );
        assert_eq!(
            after.len(),
            INHERITED_AGENT_IDENTITY_ENV.len(),
            "子进程应回报全部身份变量，实际：{after:?}"
        );
        assert!(
            after.iter().all(|value| value == "unset"),
            "clear_inherited_agent_identity 后子进程不得再看到继承身份，实际：{after:?}"
        );
        assert_ne!(
            before.first().map(String::as_str),
            after.first().map(String::as_str),
            "清除前后的 NORMA_NODE_ID 必须不同"
        );
    }

    fn child_identity_env() -> Vec<String> {
        let script = INHERITED_AGENT_IDENTITY_ENV
            .iter()
            .map(|key| format!("printf '%s\\n' \"${{{key}-unset}}\""))
            .collect::<Vec<_>>()
            .join("; ");
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", &script])
            .output()
            .expect("spawn identity env probe");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }
}

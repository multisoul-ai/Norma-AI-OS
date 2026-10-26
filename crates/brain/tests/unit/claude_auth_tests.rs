use super::*;
use serde_json::{json, Value};
use std::path::Path;

fn command(config_dir: &Path) -> Command {
    let mut command = Command::new("python3");
    command
        .arg("-c")
        .arg("import json, os; print(json.dumps({k: os.environ.get(k) for k in ['ANTHROPIC_BASE_URL', 'ANTHROPIC_AUTH_TOKEN', 'ANTHROPIC_API_KEY', 'NODE_OPTIONS', 'UNRELATED_SECRET']}))")
        .env("CLAUDE_CONFIG_DIR", config_dir)
        .env_remove("ANTHROPIC_BASE_URL")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("CLAUDE_CODE_OAUTH_TOKEN")
        .env_remove("NODE_OPTIONS")
        .env_remove("UNRELATED_SECRET");
    command
}

fn read_child_env(command: &mut Command) -> Value {
    let output = command.output().expect("启动检查环境变量的测试子进程");
    assert!(output.status.success(), "测试子进程必须正常退出");
    serde_json::from_slice(&output.stdout).expect("测试子进程应返回 JSON 环境变量")
}

/// Claude 隔离用户设置后仍能使用全局网关认证，且不会引入任意环境变量或绕过权限。
///
/// 数据构造：
///   独立配置目录仅含 settings.json，提供虚构网关 URL、测试 token、NODE_OPTIONS 和无关密钥。
///   permissions.defaultMode = bypassPermissions，模拟真实用户配置中的权限污染。
/// 执行过程：
///   1. 清除测试子进程的认证与干扰变量，指定独立 CLAUDE_CONFIG_DIR。
///   2. 配置 Claude 认证环境，启动真实 Python 子进程读取最终环境。
///   3. 检查幕僚 session_meta 的权限隔离仍然生效。
/// 预期结果：
///   - 正断言：子进程收到指定网关和测试 token，settingSources 仍为空。
///   - 负断言：没有 API key、NODE_OPTIONS 或无关密钥，不允许绕过权限。
#[test]
fn claude_gateway_credentials_survive_settings_isolation() {
    let dir = tempfile::tempdir().expect("创建独立 Claude 配置目录");
    std::fs::write(
        dir.path().join("settings.json"),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://claude-gateway.invalid",
                "ANTHROPIC_AUTH_TOKEN": "fixture-token",
                "NODE_OPTIONS": "--require unsafe.js",
                "UNRELATED_SECRET": "must-stay-private"
            },
            "permissions": {"defaultMode": "bypassPermissions"}
        })
        .to_string(),
    )
    .expect("写入包含虚构凭证的 Claude 配置");
    let mut child = command(dir.path());
    configure(&mut child, BackendKind::ClaudeCode).expect("读取网关认证配置");
    let env = read_child_env(&mut child);
    assert_eq!(
        env["ANTHROPIC_BASE_URL"], "https://claude-gateway.invalid",
        "隔离设置不能丢失网关地址"
    );
    assert_eq!(
        env["ANTHROPIC_AUTH_TOKEN"], "fixture-token",
        "隔离设置不能丢失网关 token"
    );
    assert!(
        env["ANTHROPIC_API_KEY"].is_null(),
        "未配置的 API key 不应凭空出现"
    );
    assert!(
        env["NODE_OPTIONS"].is_null(),
        "不得导入用户配置中的 Node 启动选项"
    );
    assert!(
        env["UNRELATED_SECRET"].is_null(),
        "不得将无关密钥带进幕僚进程"
    );
    let meta = crate::backend::session_meta(crate::BackendKind::ClaudeCode, None)
        .expect("Claude 会话应有权限配置");
    let options = &meta["claudeCode"]["options"];
    assert_eq!(
        options["settingSources"],
        json!([]),
        "认证恢复不能重新启用用户设置"
    );
    assert_eq!(
        options["allowDangerouslySkipPermissions"], false,
        "用户配置不能开启 bypassPermissions"
    );
}

/// 显式网关配置按整组优先，不能从用户配置补入属于其他地址的凭证。
///
/// 数据构造：配置文件包含旧网关与旧 token，子进程只显式指定新网关地址。
/// 执行过程：
///   1. 清除继承的认证变量，写入旧网关配置，显式设置新地址。
///   2. 配置认证环境，启动真实子进程观察最终环境。
/// 预期结果：
///   - 正断言：显式指定的新地址保留。
///   - 负断言：不把旧 token 或 API key 拼接给新地址，避免跨服务泄露凭证。
#[test]
fn explicit_gateway_does_not_receive_another_gateways_token() {
    let dir = tempfile::tempdir().expect("创建独立配置目录");
    std::fs::write(
        dir.path().join("settings.json"),
        json!({"env": {
            "ANTHROPIC_BASE_URL": "https://old.invalid",
            "ANTHROPIC_AUTH_TOKEN": "old-token"
        }})
        .to_string(),
    )
    .expect("写入旧网关配置");
    let mut child = command(dir.path());
    child.env("ANTHROPIC_BASE_URL", "https://explicit.invalid");
    configure(&mut child, BackendKind::ClaudeCode).expect("保留显式认证配置");
    let env = read_child_env(&mut child);
    assert_eq!(
        env["ANTHROPIC_BASE_URL"], "https://explicit.invalid",
        "显式网关地址必须优先"
    );
    assert!(
        env["ANTHROPIC_AUTH_TOKEN"].is_null(),
        "不能向新网关传递旧网关的 token"
    );
    assert!(
        env["ANTHROPIC_API_KEY"].is_null(),
        "不能向新网关补入其他来源的 API key"
    );
}

/// 非 Claude 后端不能收到 Claude 配置中的凭证。
///
/// 数据构造：独立 Claude 配置提供一对虚构网关地址与 token，目标后端为 OpenCode。
/// 执行过程：清除继承的认证变量，以 OpenCode 身份配置并启动真实子进程。
/// 预期结果：
///   - 正断言：子进程正常退出，仍可正常启动其他后端。
///   - 负断言：不注入 Claude 网关、token 或 API key。
#[test]
fn other_backends_do_not_receive_claude_credentials() {
    let dir = tempfile::tempdir().expect("创建独立配置目录");
    std::fs::write(
        dir.path().join("settings.json"),
        json!({"env": {
            "ANTHROPIC_BASE_URL": "https://claude-only.invalid",
            "ANTHROPIC_AUTH_TOKEN": "claude-only-token"
        }})
        .to_string(),
    )
    .expect("写入 Claude 专属凭证");
    let mut child = command(dir.path());
    configure(&mut child, BackendKind::OpenCode).expect("其他后端不应读取 Claude 配置");
    let env = read_child_env(&mut child);
    assert!(
        env["ANTHROPIC_BASE_URL"].is_null(),
        "OpenCode 不能继承 Claude 配置中的地址"
    );
    assert!(
        env["ANTHROPIC_AUTH_TOKEN"].is_null(),
        "OpenCode 不能继承 Claude 配置中的 token"
    );
    assert!(
        env["ANTHROPIC_API_KEY"].is_null(),
        "OpenCode 不能继承 Claude 配置中的 API key"
    );
}

/// 未指定配置目录时沿用 HOME 下的 Claude 配置，支持 API key 认证。
///
/// 数据构造：独立 HOME 下的 .claude/settings.json 仅含测试 API key。
/// 执行过程：移除 CLAUDE_CONFIG_DIR，指定独立 HOME，配置并启动真实子进程。
/// 预期结果：
///   - 正断言：收到用户配置中的 API key。
///   - 负断言：没有额外网关地址或 token，不改变默认 Anthropic 服务地址。
#[test]
fn api_key_uses_the_default_home_config_directory() {
    let dir = tempfile::tempdir().expect("创建独立 HOME");
    std::fs::create_dir(dir.path().join(".claude")).expect("创建默认 Claude 配置目录");
    std::fs::write(
        dir.path().join(".claude/settings.json"),
        json!({"env": {
            "ANTHROPIC_API_KEY": "fixture-api-key"
        }})
        .to_string(),
    )
    .expect("写入测试 API key");
    let mut child = command(dir.path());
    child
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("HOME", dir.path());
    configure(&mut child, BackendKind::ClaudeCode).expect("读取 HOME 下的默认认证配置");
    let env = read_child_env(&mut child);
    assert_eq!(
        env["ANTHROPIC_API_KEY"], "fixture-api-key",
        "默认目录中的 API key 应传入子进程"
    );
    assert!(
        env["ANTHROPIC_BASE_URL"].is_null(),
        "API key 认证不应引入其他服务地址"
    );
    assert!(
        env["ANTHROPIC_AUTH_TOKEN"].is_null(),
        "API key 认证不应引入其他 token"
    );
}

/// 相对 CLAUDE_CONFIG_DIR 必须相对于适配器工作目录读取，不能误读 App 的工作目录。
///
/// 数据构造：独立工作目录下创建名称唯一的相对配置目录，其中仅含测试 token。
/// 执行过程：指定子进程 cwd 和相对 CLAUDE_CONFIG_DIR，配置认证后启动真实子进程。
/// 预期结果：
///   - 正断言：子进程收到其工作目录内的测试 token。
///   - 负断言：没有网关地址或 API key，不从父进程工作目录补入其他认证。
#[test]
fn relative_config_directory_uses_the_child_working_directory() {
    let dir = tempfile::tempdir().expect("创建独立适配器工作目录");
    let relative = dir.path().file_name().expect("临时目录应有唯一名称");
    let config_dir = dir.path().join(relative);
    std::fs::create_dir(&config_dir).expect("创建相对配置目录");
    std::fs::write(
        config_dir.join("settings.json"),
        json!({"env": {
            "ANTHROPIC_AUTH_TOKEN": "relative-fixture-token"
        }})
        .to_string(),
    )
    .expect("写入相对目录的测试凭证");
    let mut child = command(&config_dir);
    child
        .current_dir(dir.path())
        .env("CLAUDE_CONFIG_DIR", relative);
    configure(&mut child, BackendKind::ClaudeCode).expect("读取适配器工作目录下的配置");
    let env = read_child_env(&mut child);
    assert_eq!(
        env["ANTHROPIC_AUTH_TOKEN"], "relative-fixture-token",
        "相对配置路径必须以子进程 cwd 为基准"
    );
    assert!(
        env["ANTHROPIC_BASE_URL"].is_null(),
        "不能从父进程 cwd 误读其他地址"
    );
    assert!(
        env["ANTHROPIC_API_KEY"].is_null(),
        "不能从父进程 cwd 误读其他 API key"
    );
}

/// 没有用户配置时不阻断原有 OAuth 或 Keychain 认证。
///
/// 数据构造：独立空目录，不创建 settings.json。
/// 执行过程：配置认证环境，启动真实子进程检查未注入的变量。
/// 预期结果：
///   - 正断言：没有配置文件仍能启动子进程。
///   - 负断言：不伪造网关地址、token 或 API key。
#[test]
fn missing_settings_preserve_the_default_authentication_path() {
    let dir = tempfile::tempdir().expect("创建空配置目录");
    let mut child = command(dir.path());
    configure(&mut child, BackendKind::ClaudeCode).expect("无用户设置时应保留默认认证流程");
    let env = read_child_env(&mut child);
    assert!(
        env["ANTHROPIC_BASE_URL"].is_null(),
        "没有配置就不能改变服务地址"
    );
    assert!(
        env["ANTHROPIC_AUTH_TOKEN"].is_null(),
        "没有配置就不能注入 token"
    );
    assert!(
        env["ANTHROPIC_API_KEY"].is_null(),
        "没有配置就不能注入 API key"
    );
}

/// 配置 JSON 损坏时给出可诊断错误，错误文字不泄露凭证内容。
///
/// 数据构造：未闭合的 JSON 内含仅用于测试的敏感标记。
/// 执行过程：在没有显式认证的情况下读取损坏文件，获取返回错误。
/// 预期结果：
///   - 正断言：错误类型为 InvalidData，提示 JSON 格式错误。
///   - 负断言：错误文本不包含文件中的敏感标记。
#[test]
fn malformed_settings_do_not_leak_contents_in_errors() {
    let dir = tempfile::tempdir().expect("创建配置目录");
    std::fs::write(
        dir.path().join("settings.json"),
        "{\"env\": {\"ANTHROPIC_AUTH_TOKEN\": \"private-fixture-token\"",
    )
    .expect("写入损坏 JSON");
    let error = configure(&mut command(dir.path()), BackendKind::ClaudeCode)
        .expect_err("损坏 JSON 应报告错误");
    assert_eq!(
        error.kind(),
        io::ErrorKind::InvalidData,
        "格式错误不能被伪装成正常认证配置"
    );
    assert!(
        error.to_string().contains("JSON"),
        "错误应指出配置 JSON 无效"
    );
    assert!(
        !error.to_string().contains("private-fixture-token"),
        "错误不能暴露凭证原文"
    );
}

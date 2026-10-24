//! 真实适配器与本地 API 夹具验证，不调用付费模型。

/// 默认的项目内 Read 会跳过 ACP；生产配置必须把它转成逐次权限请求。
/// 数据构造：临时项目、普通文本文件、本地 Anthropic 协议夹具与已安装的适配器。
/// 执行过程：Python 客户端分别用默认与生产规则创建真实 ACP 会话，拒绝所有权限请求。
/// 预期结果：默认读到文件中的唯一标记；生产配置恰好询问一次，拒绝结果不含文件内容。
/// 设置 NORMA_TEST_NODE、NORMA_TEST_CLAUDE_ACP 为对应可执行程序／适配器 JS 的绝对路径。
#[test]
#[ignore = "需要本机安装的 Node 和 claude-agent-acp；仅访问本地模拟 API"]
fn real_adapter_requests_permission_for_project_read() {
    let node = std::env::var("NORMA_TEST_NODE").expect("应配置测试 Node 的绝对路径");
    let adapter = std::env::var("NORMA_TEST_CLAUDE_ACP").expect("应配置真实适配器 JS 路径");
    let meta = brain::backend::session_meta(brain::BackendKind::ClaudeCode, None)
        .expect("应生成 Claude 生产选项");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/support/claude_permissions_probe.py");
    let output = std::process::Command::new("python3")
        .arg(script)
        .args(["--node", &node, "--adapter", &adapter, "--options-json"])
        .arg(meta["claudeCode"]["options"].to_string())
        .output()
        .expect("应启动本地协议测试");
    assert!(
        output.status.success(),
        "真实适配器未遵守逐次权限规则：stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

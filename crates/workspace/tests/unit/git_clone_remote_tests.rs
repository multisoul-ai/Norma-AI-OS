use super::{CloneTransport, GitHubRemote, GitHubRemoteParseError};

/// GC-R1：HTTPS、SCP 风格 SSH 与 ssh:// 地址应归一化为同一个 GitHub 仓库。
///
/// 数据构造（含关键数值的推导过程）：
///   支持的输入形式 = HTTPS(1) + 末尾斜杠 HTTPS(1) + SCP 风格 SSH(1) + ssh://(1) = 4 种。
///   每个输入都指向 owner="openai"、repository="codex"，可选 `.git` 后缀出现 3 次。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别解析四个远端地址。
///   2. 提取 owner、repository 与原始传输方式。
///   3. 从每个解析结果重新生成 canonical HTTPS 和 SSH 地址。
///
/// 预期结果：
///   - 正断言：四个输入都解析为 openai/codex。
///   - 正断言：HTTPS 输入保留 HTTPS，两个 SSH 输入归一为 SSH。
///   - 负断言：repository 不得保留 `.git` 后缀，canonical URL 不得混入另一种协议。
#[test]
fn github_remote_normalizes_supported_https_and_ssh_forms() {
    let https = GitHubRemote::parse("https://github.com/openai/codex.git")
        .expect("标准 GitHub HTTPS 地址应成功解析");
    let https_with_slash = GitHubRemote::parse("https://github.com/openai/codex.git/")
        .expect("复制自浏览器且带末尾斜杠的 GitHub HTTPS 地址应成功解析");
    let scp_ssh = GitHubRemote::parse("git@github.com:openai/codex.git")
        .expect("SCP 风格 GitHub SSH 地址应成功解析");
    let url_ssh = GitHubRemote::parse("ssh://git@github.com/openai/codex")
        .expect("ssh:// GitHub 地址应成功解析");

    assert_eq!(
        https.owner(),
        "openai",
        "HTTPS 地址的 owner 应精确解析为 openai"
    );
    assert_eq!(
        https.repository(),
        "codex",
        "HTTPS 地址的 repository 应移除 `.git` 后缀"
    );
    assert_eq!(
        https.transport(),
        CloneTransport::Https,
        "HTTPS 输入应保留 HTTPS 传输方式"
    );
    assert_eq!(
        https_with_slash, https,
        "末尾斜杠应被规范化，不得改变 owner、repository 或传输方式"
    );
    assert_eq!(scp_ssh, url_ssh, "两种 SSH 写法应归一化为同一个远端模型");
    assert_eq!(
        scp_ssh.transport(),
        CloneTransport::Ssh,
        "SCP 风格 SSH 输入应归一为 SSH 传输方式"
    );
    assert_eq!(
        https.url_for(CloneTransport::Https),
        "https://github.com/openai/codex.git",
        "canonical HTTPS 地址应包含 https scheme 与 `.git` 后缀"
    );
    assert_eq!(
        https.url_for(CloneTransport::Ssh),
        "git@github.com:openai/codex.git",
        "canonical SSH 地址应使用 GitHub SCP 风格"
    );
    assert_ne!(
        https.url_for(CloneTransport::Https),
        https.url_for(CloneTransport::Ssh),
        "HTTPS 与 SSH 的 canonical 地址不得相同"
    );
}

/// GC-R2：仅接受无歧义的 github.com 仓库地址，并返回可展示的分类错误。
///
/// 数据构造（含关键数值的推导过程）：
///   非 GitHub host(1) + 不安全凭据(1) + 缺仓库名(1) + 多余路径(1) + 不支持协议(1)
///   + 换行注入(1) = 6 个无效输入。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐个解析六个无效输入。
///   2. 在 host、用户信息、路径段和协议边界处拒绝输入。
///   3. 检查错误分类，供 UI 显示针对性提示。
///
/// 预期结果：
///   - 正断言：每类无效输入都返回对应错误。
///   - 负断言：非 GitHub 或带凭据的地址不得被当作可克隆仓库。
#[test]
fn github_remote_rejects_ambiguous_or_unsafe_addresses() {
    let wrong_host = GitHubRemote::parse("https://gitlab.com/openai/codex.git")
        .expect_err("非 github.com host 必须被拒绝");
    let embedded_credential = GitHubRemote::parse("https://token@github.com/openai/codex.git")
        .expect_err("URL 中嵌入凭据必须被拒绝，避免泄漏 secret");
    let missing_repository = GitHubRemote::parse("https://github.com/openai")
        .expect_err("缺少 repository 路径段必须被拒绝");
    let extra_path = GitHubRemote::parse("https://github.com/openai/codex/issues")
        .expect_err("仓库地址后带额外页面路径必须被拒绝");
    let unsupported_scheme = GitHubRemote::parse("git://github.com/openai/codex.git")
        .expect_err("Phase 1 未支持的 git:// 协议必须被拒绝");
    let newline_injection = GitHubRemote::parse("https://github.com/openai/codex.git\n")
        .expect_err("仓库 URL 中的换行必须被拒绝，不能被 trim 后接受");

    assert_eq!(
        wrong_host,
        GitHubRemoteParseError::UnsupportedHost,
        "非 GitHub host 应返回 UnsupportedHost"
    );
    assert_eq!(
        embedded_credential,
        GitHubRemoteParseError::EmbeddedCredentials,
        "嵌入用户信息的 HTTPS 地址应返回 EmbeddedCredentials"
    );
    assert_eq!(
        missing_repository,
        GitHubRemoteParseError::InvalidRepositoryPath,
        "缺少 repository 时应返回 InvalidRepositoryPath"
    );
    assert_eq!(
        extra_path,
        GitHubRemoteParseError::InvalidRepositoryPath,
        "多余 GitHub 页面路径应返回 InvalidRepositoryPath"
    );
    assert_eq!(
        unsupported_scheme,
        GitHubRemoteParseError::UnsupportedScheme,
        "git:// 地址应返回 UnsupportedScheme"
    );
    assert_eq!(
        newline_injection,
        GitHubRemoteParseError::InvalidFormat,
        "换行注入应返回 InvalidFormat"
    );
    assert_ne!(
        wrong_host,
        GitHubRemoteParseError::InvalidRepositoryPath,
        "错误 host 不得被误报为仓库路径问题"
    );
}

/// GC-R3：输入清理只允许首尾空白，不应放宽仓库标识符规则。
///
/// 数据构造（含关键数值的推导过程）：
///   首尾空白 = leading(2) + trailing(2) = 4 个 ASCII 空格。
///   合法 owner/repository 各 1 个；非法 owner 以连字符结尾，非法 repository 含空格。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. trim 合法 HTTPS 地址的首尾空白后解析。
///   2. 校验 owner 不以连字符开头或结尾。
///   3. 校验 repository 仅含 GitHub 仓库名允许的安全字符。
///
/// 预期结果：
///   - 正断言：带首尾空白的合法地址成功解析。
///   - 负断言：非法 owner 与含空格 repository 均被拒绝。
#[test]
fn github_remote_trims_outer_whitespace_without_accepting_invalid_names() {
    let trimmed = GitHubRemote::parse("  https://github.com/norma-os/norma_ai.git  ")
        .expect("首尾空白应在解析前被清理");
    let invalid_owner = GitHubRemote::parse("https://github.com/norma-/repo.git")
        .expect_err("以连字符结尾的 GitHub owner 必须被拒绝");
    let invalid_repository = GitHubRemote::parse("https://github.com/norma-os/bad repo.git")
        .expect_err("包含空格的 repository 必须被拒绝");

    assert_eq!(
        trimmed.owner(),
        "norma-os",
        "trim 后的 owner 应保持合法连字符"
    );
    assert_eq!(
        trimmed.repository(),
        "norma_ai",
        "trim 后的 repository 应保持合法下划线"
    );
    assert_eq!(
        invalid_owner,
        GitHubRemoteParseError::InvalidOwner,
        "非法 owner 应返回 InvalidOwner"
    );
    assert_eq!(
        invalid_repository,
        GitHubRemoteParseError::InvalidRepository,
        "含空格 repository 应返回 InvalidRepository"
    );
    assert_ne!(
        trimmed.repository(),
        "norma_ai.git",
        "解析结果不得保留 `.git` 后缀"
    );
}

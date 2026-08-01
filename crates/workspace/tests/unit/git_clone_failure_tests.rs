use super::{classify_git_failure, CloneFailureKind};

/// GC-F1：HTTPS 与 SSH 凭据失败应统一映射为 AuthenticationFailed。
///
/// 数据构造（含关键数值的推导过程）：
///   HTTPS authentication failed(1) + 无交互 username(1) + SSH publickey(1) = 3 种凭据诊断。
///   预期错误分类数 = 1（AuthenticationFailed）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将三种 Git stderr 诊断分别转为小写并匹配稳定片段。
///   2. 在 UI 文案生成前归一为 CloneFailureKind。
///   3. 比较 HTTPS 与 SSH 的恢复动作是否一致。
///
/// 预期结果：
///   - 正断言：三种诊断均为 AuthenticationFailed。
///   - 负断言：SSH key 失败不得误报为 HostVerificationFailed 或 GitFailed。
#[test]
fn git_failure_classifier_groups_https_and_ssh_authentication_errors() {
    let https = classify_git_failure("fatal: Authentication failed for repository");
    let no_prompt = classify_git_failure("fatal: could not read Username for 'https://github.com'");
    let ssh = classify_git_failure("git@github.com: Permission denied (publickey).");

    assert_eq!(
        https,
        CloneFailureKind::AuthenticationFailed,
        "HTTPS authentication failed 应映射为 AuthenticationFailed"
    );
    assert_eq!(
        no_prompt,
        CloneFailureKind::AuthenticationFailed,
        "禁用终端提示后的 username 错误应映射为 AuthenticationFailed"
    );
    assert_eq!(
        ssh,
        CloneFailureKind::AuthenticationFailed,
        "SSH publickey 拒绝应映射为 AuthenticationFailed"
    );
    assert_ne!(
        ssh,
        CloneFailureKind::HostVerificationFailed,
        "SSH key 拒绝与 host key 验证是不同恢复动作，不得混淆"
    );
    assert_ne!(
        ssh,
        CloneFailureKind::GitFailed,
        "已识别的 SSH key 拒绝不得退化为泛化 GitFailed"
    );
}

/// GC-F2：仓库、主机指纹与网络故障应保持独立分类，支持针对性恢复提示。
///
/// 数据构造（含关键数值的推导过程）：
///   repository not found(1) + host key verification(1) + DNS failure(1) + unknown fatal(1)
///   = 4 种诊断、4 个期望分类。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分类仓库不存在诊断。
///   2. 分类 SSH host key 与 DNS 网络诊断。
///   3. 将无法识别的 fatal 收口为 GitFailed。
///
/// 预期结果：
///   - 正断言：四种诊断分别命中 RepositoryNotFound、HostVerificationFailed、NetworkUnavailable、GitFailed。
///   - 负断言：网络错误不得伪装为仓库不存在。
#[test]
fn git_failure_classifier_separates_repository_host_and_network_failures() {
    let repository = classify_git_failure("remote: Repository not found.");
    let host = classify_git_failure("Host key verification failed.");
    let network =
        classify_git_failure("fatal: unable to access: Could not resolve host: github.com");
    let unknown = classify_git_failure("fatal: an unexpected git implementation error");

    assert_eq!(
        repository,
        CloneFailureKind::RepositoryNotFound,
        "Repository not found 应映射为 RepositoryNotFound"
    );
    assert_eq!(
        host,
        CloneFailureKind::HostVerificationFailed,
        "Host key verification failed 应映射为 HostVerificationFailed"
    );
    assert_eq!(
        network,
        CloneFailureKind::NetworkUnavailable,
        "DNS 解析失败应映射为 NetworkUnavailable"
    );
    assert_eq!(
        unknown,
        CloneFailureKind::GitFailed,
        "未知 fatal 应安全退化为 GitFailed"
    );
    assert_ne!(
        network,
        CloneFailureKind::RepositoryNotFound,
        "网络不可达不得误导用户去检查仓库名称或权限"
    );
}

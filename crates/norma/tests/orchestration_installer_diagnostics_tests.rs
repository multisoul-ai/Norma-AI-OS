use norma::orchestration::{
    AuthorizationRegistry, InstallPaths, OrchestrationInstaller, OrchestrationProvider,
};

#[derive(Default)]
struct MemoryAuthorizationRegistry;

impl AuthorizationRegistry for MemoryAuthorizationRegistry {
    type Error = String;

    fn authorize(
        &mut self,
        _provider: OrchestrationProvider,
        _token: &str,
        _installed_version: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn revoke(&mut self, _provider: OrchestrationProvider) -> Result<(), Self::Error> {
        Ok(())
    }

    fn is_authorized(
        &self,
        _provider: OrchestrationProvider,
        _token: &str,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

/// 缺失 bundle CLI：安装失败必须同时指出来源与用户级目标路径。
///
/// 数据构造（含关键数值的推导过程）：
///   bundled binaries = 0（CLI 与 Hook 都不存在）
///   install attempts = 1（Codex）
///   completed binary writes = 0（CLI 读取阶段立即失败）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造空 bundle 与空用户级 managed root。
///   2. 调用 install_or_repair(Codex)，Skill 可先写入。
///   3. 安装器读取缺失的 bundle/norma → 返回 NotFound。
///   4. 检查错误正文和未产生的 CLI、Hook 目标。
///
/// 预期结果：
///   - 正断言：错误包含 bundle CLI 来源路径和 managed CLI 目标路径。
///   - 正断言：错误保留底层 No such file or directory。
///   - 负断言：失败后不得伪造 managed CLI 或继续安装 Hook。
#[test]
fn missing_bundled_cli_error_names_source_and_destination() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let home = temp.path().join("home");
    let bundle = temp.path().join("bundle");
    let paths = InstallPaths::for_home_and_bundle_bin(&home, &bundle);
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry;

    let error = installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect_err("bundle CLI 缺失时安装必须失败");
    let message = error.to_string();

    assert!(
        message.contains(paths.bundled_cli.to_string_lossy().as_ref()),
        "错误必须包含缺失的 bundle CLI 来源路径，实际：{message}"
    );
    assert!(
        message.contains(paths.managed_cli_path().to_string_lossy().as_ref()),
        "错误必须包含原计划写入的 managed CLI 目标路径，实际：{message}"
    );
    assert!(
        message.contains("No such file or directory"),
        "错误必须保留底层 NotFound 原因，实际：{message}"
    );
    assert!(
        !paths.managed_cli_path().exists(),
        "bundle CLI 读取失败后不得创建空的 managed CLI"
    );
    assert!(
        !paths.managed_hook_path().exists(),
        "CLI 阶段失败后不得越过顺序继续安装 Hook"
    );
}

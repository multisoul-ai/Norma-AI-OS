//! L3 persistence acceptance against a separately built app library with its real Keychain branch.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// 正式推送配置在独立进程间恢复，明确停用和损坏配置都不能退回环境变量。
///
/// 数据构造（含关键数值的推导过程）：
///   AppDB = 2，位于新建临时目录；Keychain 项 = 1，仅属于 A 的随机安装账号。
///   凭据 = 32 字节 × 2 hex 字符 = 64 字符，全部为测试值；网络请求 = 0。
///   子进程预算 = 30s / 100ms = 300 次状态轮询，超时 kill/wait 后清理测试项。
/// 执行过程：
///   1. A 无保存配置时读取环境 fallback；新进程保存 sandbox，再由另一进程重读。
///   2. B 不继承 A；替换为 production，并在新进程重读准确地址、环境和凭据。
///   3. 停用后重启不复活；损坏测试项后重启明确失败且不覆盖；最后删除测试项。
///
/// 预期结果：
///   - 正断言：每次读写由独立的正式分支执行，准确恢复同一安装配置。
///   - 负断言：另一个安装不继承；停用/损坏不 fallback；测试结束不残留 Keychain 项。
#[test]
#[ignore = "requires NORMA_PUSH_KEYCHAIN_PROBE built with app/l3-test-support without e2e-test-support; writes one isolated Keychain item"]
fn native_push_configuration_survives_process_restarts() {
    let probe = std::env::var_os("NORMA_PUSH_KEYCHAIN_PROBE").expect("需要明确指定正式分支 runner");
    let dir = tempfile::Builder::new()
        .prefix("norma-push-keychain-l3-")
        .tempdir()
        .expect("创建隔离的推送配置目录");
    let key_file = dir.path().join("fallback.key");
    fs::write(&key_file, "a".repeat(64)).expect("写入合成 fallback 凭据");
    fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600)).expect("仅当前用户可读");
    let a = dir.path().join("a.sqlite");
    let b = dir.path().join("b.sqlite");
    let result: Result<(), String> = (|| {
        for (step, database) in [
            ("absent", &a),
            ("save-sandbox", &a),
            ("read-sandbox", &a),
            ("absent", &b),
            ("save-production", &a),
            ("read-production", &a),
            ("disable", &a),
            ("read-disabled", &a),
            ("corrupt", &a),
            ("read-corrupt", &a),
        ] {
            run(Path::new(&probe), step, database, &key_file)?;
        }
        Ok(())
    })();
    // Always attempt cleanup, including when a prior child failed; never touch a user installation.
    let cleanup = run(Path::new(&probe), "cleanup", &a, &key_file);
    if cleanup.is_err() {
        eprintln!(
            "Keychain cleanup pending; retained isolated AppDB: {}",
            dir.keep().display()
        );
    }
    assert!(cleanup.is_ok(), "隔离 Keychain 项必须清理：{cleanup:?}");
    assert!(result.is_ok(), "正式跨进程恢复验收失败：{result:?}");
}

fn run(probe: &Path, step: &str, database: &Path, key_file: &Path) -> Result<(), String> {
    let mut child = Command::new(probe)
        .arg(step)
        .arg(database)
        .env(
            "NORMA_PUSH_RELAY_URL",
            "https://fallback.invalid/v1/notifications",
        )
        .env("NORMA_PUSH_RELAY_KEY_FILE", key_file)
        .env("NORMA_PUSH_ENVIRONMENT", "sandbox")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{step}: {e}"))?;
    // 30s = 300 × 100ms; a locked Keychain must not leave an unattended process behind.
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().map_err(|e| e.to_string())?.is_none() {
        if Instant::now() >= deadline {
            child.kill().map_err(|e| e.to_string())?;
            child.wait().map_err(|e| e.to_string())?;
            return Err(format!("{step}: Keychain process timed out"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success()
        || output.stdout != format!("native-keychain-step-ok {step}\n").as_bytes()
    {
        return Err(format!(
            "{step}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

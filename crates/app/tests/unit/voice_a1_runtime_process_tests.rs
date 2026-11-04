use super::managed::prepare;
use super::*;
use std::sync::atomic::AtomicBool;

fn engine_fixture(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let source = root.join("bundle");
    let executable = source.join("engine/python/bin/python3.12");
    fs::create_dir_all(executable.parent().expect("运行时目录")).expect("引擎目录");
    let packages: BTreeMap<_, _> = asr::A1_REQUIREMENTS
        .lines()
        .filter_map(|line| line.split_once("=="))
        .collect();
    let response = serde_json::json!({"version":[3,12], "packages":packages});
    // 在操作系统子进程边界替代解释器；夹具不进行模型识别，也不作为真实App证据。
    fs::write(
        &executable,
        format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", response),
    )
    .expect("解释器夹具");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).expect("可执行权限");
    let bytes = fs::read(&executable).expect("解释器内容");
    let archive = source.join("engine.tar.bz2");
    let status = Command::new("/usr/bin/tar")
        .arg("-cjf")
        .arg(&archive)
        .arg("-C")
        .arg(&source)
        .arg("engine")
        .status()
        .expect("构造引擎归档");
    // 归档必须有统一顶层目录，生产解压器移除该层。
    assert!(status.success(), "引擎归档夹具应生成成功");
    let payload = fs::read(&archive).expect("读取归档");
    let manifest = serde_json::json!({
        "archive_bytes":payload.len(), "archive_sha256":format!("{:x}", Sha256::digest(&payload)),
        "schema":1, "minimum_macos_major":26, "target":"aarch64-apple-darwin", "python":"python/bin/python3.12",
        "requirements_sha256":format!("{:x}", Sha256::digest(asr::A1_REQUIREMENTS)),
        "files":[{"path":"python/bin/python3.12", "bytes":bytes.len(), "sha256":format!("{:x}", Sha256::digest(&bytes))}]
    });
    fs::write(
        source.join("engine.json"),
        serde_json::to_vec(&manifest).expect("引擎清单JSON"),
    )
    .expect("引擎清单");
    fs::remove_dir_all(source.join("engine")).expect("只提供发布归档，不依赖散装构建文件");
    source
}

/// 场景：普通用户没有系统Python，应用只提供已构建的可搬迁引擎包。
/// 构造：独立bundle和安装根；仅在OS子进程边界用脚本返回固定依赖元数据。
/// 执行：调用运行时安装接口，删除原bundle，再运行已安装的解释器。
/// 预期：解释器及依赖从应用包完整复制至受管目录，不引用源目录或系统Python。
#[test]
fn voice_a1_managed_engine_installs_without_a_user_python() {
    let temp = tempfile::tempdir().expect("隔离引擎目录");
    let source = engine_fixture(temp.path());
    let root = temp.path().join("models");
    let python = prepare(&root, &source, &AtomicBool::new(false)).expect("应用自带引擎应可安装");
    assert!(
        python.starts_with(root.join("a1/engines")),
        "解释器应属于应用受管目录"
    );
    assert!(
        !python.starts_with(&source),
        "不可把应用包位置当成安装后的运行依赖"
    );
    fs::remove_dir_all(source).expect("移走源引擎包");
    assert!(
        packages_match(&python, &AtomicBool::new(false)).expect("已安装环境可执行"),
        "移走构建目录后仍能读取完整依赖"
    );
}

/// 场景：引擎包损坏时不得执行其中程序；已有缓存和配置必须保留，换回正确包可重试。
/// 构造：一个受管安装，随后把源归档替换为同长度坏字节，并损坏已安装解释器。
/// 执行：安装坏归档，再重新构造正确发布包重试。
/// 预期：失败明确归因为完整性错误，旧指针逐字保留；重试发布新目录，不删除旧目录。
#[test]
fn voice_a1_engine_integrity_failure_preserves_cache_and_allows_retry() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let source = engine_fixture(temp.path());
    let root = temp.path().join("models");
    let cancelled = AtomicBool::new(false);
    let previous = prepare(&root, &source, &cancelled).expect("原安装");
    let cached = root.join("a1/python-runtime.json");
    let before = fs::read(&cached).expect("原缓存");
    let archive = source.join("engine.tar.bz2");
    let good = fs::read(&archive).expect("正确发布包");
    fs::write(&archive, vec![0; good.len()]).expect("损坏包");
    fs::write(&previous, b"damaged").expect("模拟已安装文件损坏");
    assert!(
        matches!(
            prepare(&root, &source, &cancelled),
            Err(A1InstallFailure::EngineIntegrity)
        ),
        "坏归档不能启动解释器或发布缓存"
    );
    assert_eq!(
        fs::read(&cached).expect("原缓存"),
        before,
        "失败必须保留原指针"
    );
    fs::write(archive, good).expect("恢复正确发布包");
    let repaired = prepare(&root, &source, &cancelled).expect("真实归档重试");
    assert_ne!(
        repaired, previous,
        "修复必须发布新目录，不能覆盖可能仍在使用的版本"
    );
    assert!(previous.exists(), "修复不得删除其他会话可能使用的旧目录");
    assert!(
        packages_match(&repaired, &cancelled).expect("修复后可执行"),
        "新解释器应可实际运行"
    );
}

/// 场景：用户在安装前取消，不应留下任何新引擎或缓存。
/// 构造：有效的发布包和已置位取消标记；执行完整运行时准备接口。
/// 预期：返回Cancelled，安装根尚不存在，也不能执行包内解释器。
#[test]
fn voice_a1_cancelled_engine_does_not_publish_files() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let source = engine_fixture(temp.path());
    let root = temp.path().join("models");
    assert!(
        matches!(
            prepare(&root, &source, &AtomicBool::new(true)),
            Err(A1InstallFailure::Cancelled)
        ),
        "取消必须返回明确状态"
    );
    assert!(!root.exists(), "取消不能提交引擎目录或缓存");
}

/// 场景：缓存被改写为包清单之外的另一个可执行文件时，不能执行该文件。
/// 构造：先安装有效包，再放入能返回相同依赖JSON但会创建marker的额外脚本。
/// 执行：将缓存指向额外脚本后重新准备运行环境。
/// 预期：只运行清单指定的解释器；marker不存在，最终路径仍为python3.12。
/// 另将已安装解释器移除执行权限；内容哈希不变时也应自动修复，而非永远报错。
#[test]
fn voice_a1_cached_executable_must_match_the_verified_manifest() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().expect("隔离缓存目录");
    let source = engine_fixture(temp.path());
    let root = temp.path().join("models");
    let cancelled = AtomicBool::new(false);
    let python = prepare(&root, &source, &cancelled).expect("原受管安装");
    let substituted = python.with_file_name("unlisted-executable");
    let marker = temp.path().join("unverified-was-executed");
    let response = fs::read_to_string(&python).expect("解释器夹具");
    fs::write(
        &substituted,
        response.replacen(
            "#!/bin/sh\n",
            &format!("#!/bin/sh\n/usr/bin/touch '{}'\n", marker.display()),
            1,
        ),
    )
    .expect("清单外脚本");
    fs::set_permissions(&substituted, fs::Permissions::from_mode(0o755)).expect("脚本权限");
    save_cache(&root.join("a1/python-runtime.json"), &substituted).expect("改写缓存");
    let actual = prepare(&root, &source, &cancelled).expect("安全恢复");
    assert!(!marker.exists(), "不能在校验前执行缓存指定的清单外文件");
    assert_eq!(
        actual.file_name().expect("解释器名"),
        "python3.12",
        "只可选择清单固定解释器"
    );
    assert_ne!(actual, substituted, "不能接受缓存覆盖的解释器路径");
    fs::set_permissions(&actual, fs::Permissions::from_mode(0o644)).expect("模拟丢失执行权限");
    let repaired = prepare(&root, &source, &cancelled).expect("无执行权限的缓存应自动修复");
    assert_ne!(repaired, actual, "不能一直复用无法执行的受管缓存");
    assert!(
        packages_match(&repaired, &cancelled).expect("修复环境可执行"),
        "修复后的解释器必须能够真实启动"
    );
}

/// 场景：运行环境缓存更新必须原子替换，不能截断旧缓存指向的文件。
/// 构造：缓存位置为指向旧配置文件的符号链接，新Python路径无需执行。
/// 执行：通过缓存发布接口保存新路径，检查旧目标与新缓存。
/// 预期：旧内容逐字保留，新缓存为普通文件并可反序列化出完整路径。
#[test]
fn voice_a1_runtime_cache_replaces_pointer_without_truncating_previous_target() {
    let temp = tempfile::tempdir().expect("隔离缓存目录");
    let previous = temp.path().join("previous.json");
    let cached = temp.path().join("python-runtime.json");
    fs::write(&previous, b"previous-configuration").expect("旧配置");
    std::os::unix::fs::symlink(&previous, &cached).expect("旧缓存链接");
    let python = temp.path().join("runtime/bin/python");
    save_cache(&cached, &python).expect("原子保存缓存");
    assert_eq!(
        fs::read(&previous).expect("旧目标"),
        b"previous-configuration",
        "发布新缓存不能经旧链接截断其他配置"
    );
    assert!(
        !fs::symlink_metadata(&cached)
            .expect("新缓存")
            .file_type()
            .is_symlink(),
        "新缓存应原子替换旧链接"
    );
    assert_eq!(
        serde_json::from_slice::<PathBuf>(&fs::read(cached).expect("新内容")).expect("完整JSON"),
        python,
        "发布的缓存必须完整保存新运行环境路径"
    );
}

/// 场景：主进程已退出、后代仍持有stdout时，运行器也必须遵守超时并收口进程组。
/// 构造：shell立即退出，sleep后代持有管道4秒；执行预算1秒，调度允许额外2秒。
/// 执行：调用运行器并测量实际耗时。
/// 预期：返回失败且小于3秒，不能在join管道线程时失去超时保护并等待4秒后报成功。
#[test]
fn voice_a1_runtime_deadline_includes_descendant_pipe_lifetime() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 4 & exit 0"]);
    let start = Instant::now();
    let result = run(command, 1, &AtomicBool::new(false));
    assert!(
        result.is_err(),
        "后代管道超过预算必须报告失败，不能把主进程退出当成全部完成"
    );
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "管道收集也必须有界，不能无条件join到后代退出"
    );
}

#[path = "voice_a1_remote_tests.rs"]
mod remote;

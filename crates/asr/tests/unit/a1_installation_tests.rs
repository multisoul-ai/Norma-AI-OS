use super::*;

/// 场景：从页面提交不完整的A1配置不能破坏上一份已保存的安装配置。
/// 构造：隔离目录保存旧manifest；新配置缺少Python和模型文件，协议为现行v1。
/// 执行：调用公开安装配置接口，再读取既有manifest并尝试读取缺失的新模型。
/// 预期：安装返回错误，旧manifest逐字保持不变，没有伪造成功配置。
#[test]
fn a1_install_rejects_missing_files_without_replacing_saved_configuration() {
    let temp = tempfile::tempdir().expect("隔离安装目录");
    std::fs::create_dir(temp.path().join("a1")).expect("配置目录");
    let manifest = temp.path().join("a1/runtime.json");
    std::fs::write(&manifest, b"previous-installation").expect("旧配置");
    let config = A1Config {
        protocol: 1,
        python: temp.path().join("missing-python"),
        python_sha256: "0".repeat(64),
        zipformer: temp.path().join("zipformer"),
        qwen: temp.path().join("qwen"),
    };
    assert!(
        config.install(temp.path()).is_err(),
        "缺失的模型与Python不能提交安装"
    );
    assert_eq!(
        std::fs::read(&manifest).expect("旧配置仍可读"),
        b"previous-installation",
        "失败不得替换上一份配置"
    );
    assert!(!config.qwen.exists(), "失败不得制造已安装模型状态");
}

/// 场景：最终提交门只可在完整性检查通过后调用，不能用一次预检代替最终取消检查。
/// 构造：不完整A1配置与用于记录调用顺序的Cell，不需要加载任何模型。
/// 执行：调用带最终提交门的安装入口。
/// 预期：先因完整性失败返回，提交门未调用；不能提前消耗最终授权再执行长校验。
#[test]
fn a1_commit_gate_is_not_consumed_before_validation() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let called = std::cell::Cell::new(false);
    let config = A1Config {
        protocol: 1,
        python: temp.path().join("missing"),
        python_sha256: "0".repeat(64),
        zipformer: temp.path().join("zipformer"),
        qwen: temp.path().join("qwen"),
    };
    let result = config.install_with_gate(temp.path(), || {
        called.set(true);
        Ok(())
    });
    assert!(result.is_err(), "缺失文件必须拒绝安装");
    assert!(!called.get(), "不能在长校验开始前提前消费最终提交门");
}

/// 场景：真实模型完整校验后，取消门仍须能阻止manifest替换。
/// 构造：只读本机已安装的A1模型和Python，隔离目录保存有效的旧配置；不启动模型进程。
/// 执行：安装走完实际文件校验，在最后提交门返回取消，再逐字比较旧配置。
/// 预期：返回失败且提交门确实运行；旧配置保持不变，没有把最终校验当成不可取消阶段。
#[test]
#[ignore = "Requires the locally installed, pinned A1 model files; no inference"]
fn a1_cancel_at_real_commit_gate_preserves_previous_manifest() {
    let config =
        A1Config::from_model_root(&crate::default_model_installation_root().expect("本机模型根"))
            .expect("已有真实A1配置");
    let temp = tempfile::tempdir().expect("隔离目录");
    std::fs::create_dir(temp.path().join("a1")).expect("配置目录");
    let path = temp.path().join("a1/runtime.json");
    let previous = serde_json::to_vec(&config).expect("有效旧配置");
    std::fs::write(&path, &previous).expect("写入旧配置");
    let called = std::cell::Cell::new(false);
    let result = config.install_with_gate(temp.path(), || {
        called.set(true);
        Err(error("installation cancelled"))
    });
    assert!(called.get(), "真实文件校验后必须调用最终取消门");
    assert!(result.is_err(), "最终取消必须拒绝配置替换");
    assert_eq!(
        std::fs::read(path).expect("原配置"),
        previous,
        "取消后原配置必须逐字保留"
    );
}

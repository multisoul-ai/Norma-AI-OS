use crate::{
    install_archive, install_archive_with_support, InstallationError, ModelArtifact,
    SupplementalFile, INSTALLATION_MANIFEST_FILE,
};
use bzip2::write::BzEncoder;
use bzip2::Compression;
use sha2::{Digest, Sha256};
use std::fs;
use tar::{Builder, Header};

/// MODEL-STORE-1：校验通过的归档必须原子成为 Ready，错误哈希不得留下模型目录。
///
/// 数据构造（含关键数值的推导过程）：
///   required files = 2（model.onnx + tokens.txt）
///   payload bytes  = 5 + 6 = 11 bytes
///   expected hash  = SHA-256(测试归档) = 64 个十六进制字符
///   invalid hash   = 64 个 '0'，与实际归档哈希不同
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造带顶层 bundle 的 tar.bz2 测试归档；
///   2. 使用正确哈希安装，检查最终目录和清单；
///   3. 使用错误哈希安装到独立根目录，检查没有 Ready 目录。
///
/// 预期结果：
///   - 正断言：正确归档获得两个必要文件与 installation.json；
///   - 负断言：错误哈希不产生最终模型目录或清单。
#[test]
fn test_install_archive_is_atomic_and_rejects_hash_mismatch() {
    let archive = test_archive();
    let digest = format!("{:x}", Sha256::digest(&archive));
    let temp = tempfile::tempdir().expect("应能创建 model_store 测试目录");
    let archive_path = temp.path().join("voice.tar.bz2");
    fs::write(&archive_path, archive).expect("应能写入测试模型归档");
    let artifact = ModelArtifact::new(
        "https://example.invalid/voice.tar.bz2",
        &digest,
        11,
        vec!["model.onnx", "tokens.txt"],
    );

    let available = install_archive(
        "test-voice-v1",
        &artifact,
        &archive_path,
        &temp.path().join("models"),
    )
    .expect("哈希和必需文件正确时应原子安装");

    assert!(
        available.root().join("model.onnx").is_file(),
        "Ready 安装应包含从归档解出的 model.onnx"
    );
    assert!(
        available.root().join("tokens.txt").is_file(),
        "Ready 安装应包含从归档解出的 tokens.txt"
    );
    assert!(
        available.root().join(INSTALLATION_MANIFEST_FILE).is_file(),
        "Ready 安装必须最后写入 installation.json"
    );

    let invalid_root = temp.path().join("invalid-models");
    let invalid = ModelArtifact::new(
        "https://example.invalid/voice.tar.bz2",
        "0".repeat(64),
        11,
        vec!["model.onnx", "tokens.txt"],
    );
    let result = install_archive("test-voice-v1", &invalid, &archive_path, &invalid_root);

    assert!(result.is_err(), "错误哈希的归档必须被拒绝");
    assert!(
        !invalid_root.join("test-voice-v1").exists(),
        "错误哈希不得产生可见的最终模型目录"
    );
    assert!(
        !invalid_root
            .join("test-voice-v1")
            .join(INSTALLATION_MANIFEST_FILE)
            .exists(),
        "错误哈希不得留下会被扫描成 Ready 的安装清单"
    );
}

/// MODEL-STORE-2：模型附属文件必须在同一个临时安装中校验并原子进入 Ready 目录。
///
/// 数据构造（含关键数值的推导过程）：
///   archive required files = 2（model.onnx + tokens.txt）
///   support files          = 1（vocoder.onnx）
///   final required files   = 2 + 1 = 3
///   support hash           = SHA-256("vocoder") = 64 个十六进制字符
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造通过哈希校验的主归档和单个 raw support 文件；
///   2. 以 `install_archive_with_support` 安装到受控根目录；
///   3. 检查 support 文件和安装清单在同一个最终目录中出现。
///
/// 预期结果：
///   - 正断言：vocoder.onnx 与主模型文件同时存在；
///   - 负断言：临时下载源不得被移动或误当作最终模型根。
#[test]
fn test_install_archive_with_support_promotes_all_required_artifacts_together() {
    let archive = test_archive();
    let archive_digest = format!("{:x}", Sha256::digest(&archive));
    let support = b"vocoder";
    let support_digest = format!("{:x}", Sha256::digest(support));
    let temp = tempfile::tempdir().expect("应能创建附属文件测试目录");
    let archive_path = temp.path().join("voice.tar.bz2");
    let support_path = temp.path().join("vocoder-download.onnx");
    fs::write(&archive_path, archive).expect("应能写主模型归档");
    fs::write(&support_path, support).expect("应能写 vocoder 测试文件");
    let artifact = ModelArtifact::new(
        "https://example.invalid/voice.tar.bz2",
        archive_digest,
        11,
        vec!["model.onnx", "tokens.txt"],
    );
    let support_file = SupplementalFile::new(&support_path, support_digest, "vocoder.onnx");

    let available = install_archive_with_support(
        "test-voice-v1",
        &artifact,
        &archive_path,
        &temp.path().join("models"),
        &[support_file],
    )
    .expect("主归档和附属文件完整时应一起成为 Ready");

    assert!(
        available.root().join("vocoder.onnx").is_file(),
        "附属 vocoder 必须与主模型一起进入最终受控目录"
    );
    assert!(
        available.root().join(INSTALLATION_MANIFEST_FILE).is_file(),
        "附属文件写入成功后才应产生 Ready 清单"
    );
    assert!(
        support_path.is_file(),
        "下载缓存源文件应保留给调用方按成功路径清理，不能被安装器误移动"
    );
    assert_ne!(
        available.root(),
        support_path.as_path(),
        "最终受控模型根不得等于下载缓存文件路径"
    );
}

/// MODEL-STORE-3：模型 ID 必须是一个受控目录名，不能穿越安装根目录。
///
/// 数据构造（含关键数值的推导过程）：
///   model id components = 2（`..` + `outside`，不是单一普通目录名）
///   required files      = 2（model.onnx + tokens.txt）
///   escape targets      = 2（模型根外的 outside 与安装根内的同名目录）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个哈希和必需文件都正确的最小归档；
///   2. 以 `../outside` 作为模型 ID 请求安装；
///   3. 检查 API 返回路径安全错误，并检查两类候选 Ready 目录。
///
/// 预期结果：
///   - 断言 A：路径穿越 ID 必须被显式拒绝；
///   - 断言 B：安装根外不得出现 outside 目录；
///   - 断言 C：安装根内也不得把非法 ID 当作可用模型创建目录。
#[test]
fn test_install_archive_rejects_model_id_path_traversal() {
    let archive = test_archive();
    let digest = format!("{:x}", Sha256::digest(&archive));
    let temp = tempfile::tempdir().expect("应创建路径安全测试目录");
    let archive_path = temp.path().join("voice.tar.bz2");
    fs::write(&archive_path, archive).expect("应写入路径安全测试归档");
    let artifact = ModelArtifact::new(
        "https://example.invalid/voice.tar.bz2",
        digest,
        11,
        vec!["model.onnx", "tokens.txt"],
    );
    let install_root = temp.path().join("models");

    let result = install_archive("../outside", &artifact, &archive_path, &install_root);

    assert!(
        matches!(result, Err(InstallationError::UnsafeModelId(_))),
        "包含 `..` 的模型 ID 必须返回路径安全错误，而不是进入安装流程"
    );
    assert!(
        !temp.path().join("outside").exists(),
        "非法模型 ID 不得在受控安装根外创建 outside 目录"
    );
    assert!(
        !install_root.join("outside").exists(),
        "非法模型 ID 不得在受控安装根内伪装成合法 Ready 目录"
    );
}

fn test_archive() -> Vec<u8> {
    let encoder = BzEncoder::new(Vec::new(), Compression::best());
    let mut tar = Builder::new(encoder);
    append_file(&mut tar, "bundle/model.onnx", b"model");
    append_file(&mut tar, "bundle/tokens.txt", b"tokens");
    tar.into_inner()
        .expect("测试 tar 应能结束")
        .finish()
        .expect("测试 bzip2 应能结束")
}

fn append_file(tar: &mut Builder<BzEncoder<Vec<u8>>>, path: &str, contents: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, path, contents)
        .expect("测试 tar 应能写入文件");
}

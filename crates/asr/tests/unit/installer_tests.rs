use crate::{
    install_model_archive, AsrArtifact, AsrCapabilities, AsrModelDescriptor, AsrModelId,
    INSTALLATION_MANIFEST_FILE,
};
use bzip2::write::BzEncoder;
use bzip2::Compression;
use sha2::{Digest, Sha256};
use std::fs;
use tar::{Builder, Header};

const TEST_MODEL_ID: &str = "test-streaming-zh-int8-2026-07-11";

/// ASR-INSTALLER-1：安装器必须验证归档哈希并以原子目录产生 Ready 安装。
///
/// 数据构造（含关键数值的推导过程）：
///   required file count = 3，encoder + decoder + tokens
///   file payload bytes  = 3 + 3 + 6 = 12 bytes
///   archive layout      = bundle/<required file>，验证顶层目录可被去除
///   valid hash length   = 64 个十六进制字符
///   invalid hash        = 64 个 '0'，与真实归档哈希不同
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 生成只含 3 个必要文件的 tar.bz2 归档并写入临时文件
///   2. 使用真实哈希安装 → 校验、提取、写清单、原子 rename
///   3. 使用错误哈希安装到另一根目录 → 在解压前失败
///
/// 预期结果：
///   - 正断言：成功安装只包含必要文件与 installation.json
///   - 负断言：错误哈希不产生模型目录或安装清单
#[test]
fn test_install_archive_is_atomic_and_rejects_hash_mismatch() {
    let archive = make_test_archive();
    let digest = format!("{:x}", Sha256::digest(&archive));
    let temp = tempfile::tempdir().expect("应可创建安装器临时根目录");
    let archive_path = temp.path().join("model.tar.bz2");
    fs::write(&archive_path, &archive).expect("应可写入测试归档");
    let model_root = temp.path().join("models");
    let descriptor = test_descriptor(&digest);

    let available = install_model_archive(&descriptor, &archive_path, &model_root)
        .expect("哈希与文件清单正确时安装应成功");
    let installed_root = model_root.join(TEST_MODEL_ID);

    assert_eq!(
        available.root(),
        installed_root.as_path(),
        "安装成功后应以不可变模型 ID 作为受管目录"
    );
    assert!(
        installed_root.join("encoder.int8.onnx").is_file(),
        "安装目录应包含 encoder"
    );
    assert!(
        installed_root.join("decoder.int8.onnx").is_file(),
        "安装目录应包含 decoder"
    );
    assert!(
        installed_root.join("tokens.txt").is_file(),
        "安装目录应包含 tokens"
    );
    assert!(
        installed_root.join(INSTALLATION_MANIFEST_FILE).is_file(),
        "原子安装必须在成功后写入 installation.json"
    );

    let invalid_root = temp.path().join("invalid-models");
    let invalid_descriptor = test_descriptor(&"0".repeat(64));
    let invalid_result = install_model_archive(&invalid_descriptor, &archive_path, &invalid_root);

    assert!(
        invalid_result.is_err(),
        "归档哈希与受控清单不同时安装必须失败"
    );
    assert!(
        !invalid_root.join(TEST_MODEL_ID).exists(),
        "错误哈希不应产生最终模型目录"
    );
    assert!(
        !invalid_root
            .join(TEST_MODEL_ID)
            .join(INSTALLATION_MANIFEST_FILE)
            .exists(),
        "错误哈希不应产生可被误认为 Ready 的安装清单"
    );
}

/// ASR-INSTALLER-2：缺少必要文件的归档不得产生最终安装或清单。
///
/// 数据构造（含关键数值的推导过程）：
///   required files = 3（encoder + decoder + tokens）
///   archived files = 2（encoder + decoder）
///   missing files  = 3 - 2 = 1（tokens.txt）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 生成哈希正确但缺少 tokens.txt 的 tar.bz2
///   2. install_model_archive 解压到临时目录并校验必要文件
///   3. 校验失败后检查最终模型 ID 目录与 installation.json
///
/// 预期结果：
///   - 正断言：返回 MissingRequiredFile(tokens.txt)
///   - 负断言：最终目录和清单均不存在，不能污染 Ready 安装
#[test]
fn test_install_archive_missing_required_file_never_becomes_ready() {
    let encoder = BzEncoder::new(Vec::new(), Compression::best());
    let mut tar = Builder::new(encoder);
    append_file(&mut tar, "bundle/encoder.int8.onnx", b"enc");
    append_file(&mut tar, "bundle/decoder.int8.onnx", b"dec");
    let encoder = tar.into_inner().expect("应完成缺文件测试 tar");
    let archive = encoder.finish().expect("应完成缺文件 bzip2");
    let digest = format!("{:x}", Sha256::digest(&archive));
    let temp = tempfile::tempdir().expect("应创建缺文件测试目录");
    let archive_path = temp.path().join("missing.tar.bz2");
    fs::write(&archive_path, archive).expect("应写入缺文件归档");
    let model_root = temp.path().join("models");

    let result = install_model_archive(&test_descriptor(&digest), &archive_path, &model_root);
    let final_root = model_root.join(TEST_MODEL_ID);

    assert!(
        matches!(
            &result,
            Err(crate::AsrInstallationError::MissingRequiredFile(path))
                if path.ends_with("tokens.txt")
        ),
        "缺少 tokens.txt 应返回精确 MissingRequiredFile，实际结果: {result:?}"
    );
    assert!(!final_root.exists(), "必要文件不完整时不得创建最终模型目录");
    assert!(
        !final_root.join(INSTALLATION_MANIFEST_FILE).exists(),
        "必要文件不完整时不得留下 Ready 清单"
    );
}

/// ASR-INSTALLER-3：归档中的 `..` 路径不得逃逸受管临时目录。
///
/// 数据构造（含关键数值的推导过程）：
///   required files = 3（encoder + decoder + tokens）
///   traversal depth = 2（`../../escaped/tokens.txt`）
///   escaped writes expected = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 生成两个正常模型条目和一个带两级 `..` 的 tokens 条目
///   2. 安装器只按受控必要文件 basename 写入临时根
///   3. 检查最终 tokens 与模型根外的 escaped 路径
///
/// 预期结果：
///   - 正断言：受控安装根得到 tokens.txt 并能完成安装
///   - 负断言：`escaped/tokens.txt` 不得在模型根外出现
#[test]
fn test_install_archive_confines_traversal_entry_to_managed_root() {
    let encoder = BzEncoder::new(Vec::new(), Compression::best());
    let mut tar = Builder::new(encoder);
    append_file(&mut tar, "bundle/encoder.int8.onnx", b"enc");
    append_file(&mut tar, "bundle/decoder.int8.onnx", b"dec");
    append_raw_path_file(&mut tar, "../../escaped/tokens.txt", b"tokens");
    let encoder = tar.into_inner().expect("应完成路径穿越测试 tar");
    let archive = encoder.finish().expect("应完成路径穿越测试 bzip2");
    let digest = format!("{:x}", Sha256::digest(&archive));
    let temp = tempfile::tempdir().expect("应创建路径穿越测试目录");
    let archive_path = temp.path().join("traversal.tar.bz2");
    fs::write(&archive_path, archive).expect("应写入路径穿越测试归档");
    let model_root = temp.path().join("models");

    let available = install_model_archive(&test_descriptor(&digest), &archive_path, &model_root)
        .expect("安全 basename 提取应允许受控必要文件完成安装");

    assert!(
        available.root().join("tokens.txt").is_file(),
        "路径穿越条目的 basename 应只写到受管安装根"
    );
    assert!(
        !temp.path().join("escaped/tokens.txt").exists(),
        "两级 .. 条目不得在模型根外创建 escaped/tokens.txt"
    );
    assert_ne!(
        available.root(),
        temp.path().join("escaped").as_path(),
        "可用模型根不得被归档路径重定向到 escaped 目录"
    );
}

fn test_descriptor(sha256: &str) -> AsrModelDescriptor {
    AsrModelDescriptor::new(
        AsrModelId::parse(TEST_MODEL_ID).expect("测试模型 ID 应有效"),
        "Test Streaming ZH",
        AsrCapabilities::new(vec!["zh"], true),
        AsrArtifact::new(
            "https://example.invalid/model.tar.bz2",
            sha256.to_string(),
            0,
            vec!["encoder.int8.onnx", "decoder.int8.onnx", "tokens.txt"],
        ),
    )
}

fn make_test_archive() -> Vec<u8> {
    let encoder = BzEncoder::new(Vec::new(), Compression::best());
    let mut tar = Builder::new(encoder);
    append_file(&mut tar, "bundle/encoder.int8.onnx", b"enc");
    append_file(&mut tar, "bundle/decoder.int8.onnx", b"dec");
    append_file(&mut tar, "bundle/tokens.txt", b"tokens");
    let encoder = tar.into_inner().expect("应可完成测试 tar 归档");
    encoder.finish().expect("应可完成 bzip2 压缩")
}

fn append_file(builder: &mut Builder<BzEncoder<Vec<u8>>>, path: &str, bytes: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_size(bytes.len() as u64);
    header.set_cksum();
    builder
        .append_data(&mut header, path, bytes)
        .expect("应可将必要模型文件写入测试归档");
}

fn append_raw_path_file(builder: &mut Builder<BzEncoder<Vec<u8>>>, path: &str, bytes: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_size(bytes.len() as u64);
    header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
    header.set_cksum();
    builder
        .append(&header, bytes)
        .expect("应可构造不经过 Builder 路径清洗的恶意测试条目");
}

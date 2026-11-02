use crate::{install_verified_files, validate_verified_files, VerifiedFile};
use std::fs;

/// 场景：归档安装有自己的manifest，仍需独立验证每个引擎文件的实际内容。
/// 构造：三字节abc与公开SHA256向量；不伪造另一种安装manifest。
/// 执行：通过内容校验接口检查正确文件，再改为同长度的bad重查。
/// 预期：正确内容通过，损坏内容拒绝，校验不新增或改写安装记录。
#[test]
fn verified_contents_work_without_replacing_archive_manifest() {
    let temp = tempfile::tempdir().expect("隔离归档目录");
    fs::write(temp.path().join("library"), b"abc").expect("正确库文件");
    let files = [VerifiedFile::new(
        "library",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )];
    assert!(
        crate::validate_file_contents(&files, temp.path()).is_ok(),
        "独立内容校验应接受正确文件"
    );
    fs::write(temp.path().join("library"), b"bad").expect("损坏文件");
    assert!(
        crate::validate_file_contents(&files, temp.path()).is_err(),
        "不能只校验存在或长度"
    );
    assert!(
        !temp.path().join("installation.json").exists(),
        "只读校验不能生成另一种安装记录"
    );
}

/// 场景：下载内容损坏时不得提交安装；同一次请求使用正确内容重试可安装。
/// 构造：临时下载目录中写入bad；可信目录约定3字节abc及其公开SHA256测试向量。
/// 执行：安装损坏文件后检查目标不存在，再换成abc重试并篡改安装内容重新校验。
/// 预期：坏文件不能形成可用安装，正确重试原子提交；安装后损坏必须被再次校验拦截。
#[test]
fn verified_files_reject_corruption_and_allow_clean_retry() {
    let temp = tempfile::tempdir().expect("临时目录");
    let source = temp.path().join("downloads");
    let models = temp.path().join("models");
    fs::create_dir(&source).expect("下载目录");
    fs::write(source.join("weights"), b"bad").expect("损坏下载");
    let files = [VerifiedFile::new(
        "weights",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )];
    let failed = install_verified_files("test-model", &files, &source, &models);
    assert!(failed.is_err(), "哈希错误必须拒绝安装");
    assert!(
        !models.join("test-model").exists(),
        "失败不得暴露半安装目录"
    );
    fs::write(source.join("weights"), b"abc").expect("修复下载");
    let installed =
        install_verified_files("test-model", &files, &source, &models).expect("正确内容重试成功");
    assert_eq!(
        fs::read(installed.root().join("weights")).expect("读取"),
        b"abc",
        "安装内容必须匹配可信测试向量"
    );
    assert!(
        validate_verified_files("test-model", &files, installed.root()).is_ok(),
        "安装可重新加载并完整校验"
    );
    fs::write(installed.root().join("weights"), b"bad").expect("模拟安装后损坏");
    assert!(
        validate_verified_files("test-model", &files, installed.root()).is_err(),
        "不能仅凭安装标记把已损坏内容当成可用模型"
    );
}

/// 场景：模型清单不能把自身的安装manifest声明成模型文件，导致提交后覆盖已校验内容。
/// 构造：下载内容abc位于保留路径installation.json；字节数和SHA256本来完全正确。
/// 执行：通过公开安装接口提交该目录。
/// 预期：拒绝清单，不产生成功安装；不能先返回成功、随后完整性复查才失败。
#[test]
fn verified_files_reject_installation_manifest_collision() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let source = temp.path().join("source");
    std::fs::create_dir(&source).expect("源目录");
    std::fs::write(source.join("installation.json"), b"abc").expect("模型内容");
    let files = [VerifiedFile::new(
        "installation.json",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )];
    let result = install_verified_files("collision", &files, &source, &temp.path().join("models"));
    assert!(
        result.is_err(),
        "保留路径必须在提交前拒绝，不能覆盖模型文件后返回成功"
    );
    assert!(
        !temp.path().join("models/collision").exists(),
        "清单冲突不能产生安装目录"
    );
}

/// 场景：大小写不敏感文件系统也不能让模型文件覆盖安装manifest。
/// 构造：合法abc内容使用保留名的大写形式INSTALLATION.JSON。
/// 执行：通过公开安装入口提交。
/// 预期：所有平台一致拒绝该别名，避免macOS上安装后才出现完整性损坏。
#[test]
fn verified_files_reject_manifest_case_alias() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let source = temp.path().join("source");
    fs::create_dir(&source).expect("源目录");
    fs::write(source.join("INSTALLATION.JSON"), b"abc").expect("文件");
    let files = [VerifiedFile::new(
        "INSTALLATION.JSON",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )];
    assert!(
        install_verified_files("case-alias", &files, &source, &temp.path().join("models")).is_err(),
        "manifest大小写别名必须在安装前拒绝"
    );
    assert!(
        !temp.path().join("models/case-alias").exists(),
        "不得提交与manifest冲突的模型目录"
    );
}

/// 场景：损坏的目录元数据包含总字节数溢出时，安装应返回错误而非panic。
/// 构造：两个独立文件声明u64::MAX字节，和超出u64范围；源内容无需读取。
/// 执行：调用公开安装接口。
/// 预期：目录在内容处理前被拒绝，目标未产生；不能因元数据算术错误崩溃。
#[test]
fn verified_files_reject_total_size_overflow() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let files = [
        VerifiedFile::new("one", u64::MAX, "0".repeat(64)),
        VerifiedFile::new("two", u64::MAX, "0".repeat(64)),
    ];
    assert!(
        install_verified_files("overflow", &files, temp.path(), &temp.path().join("models"))
            .is_err(),
        "总大小溢出必须返回错误"
    );
    assert!(
        !temp.path().join("models/overflow").exists(),
        "损坏元数据不得形成安装"
    );
}

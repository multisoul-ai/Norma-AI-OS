use crate::{
    builtin_model_catalog, remove_installation, validate_installation, AsrModelId,
    InstallationManifest, INSTALLATION_MANIFEST_FILE, PARAFORMER_MODEL_ID,
};

/// ASR-INSTALL-ROOT-1：受管 ASR 模型必须落在应用数据根下的稳定专用目录。
///
/// 数据构造（含关键数值的推导过程）：
///   application data root = /tmp/Norma OS（测试注入，不访问真实用户目录）
///   appended segments     = 1，固定追加 models
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将注入的应用数据目录传入 model_installation_root
///   2. 读取返回路径，不创建目录也不扫描文件系统
///
/// 预期结果：
///   - 正断言：返回 /tmp/Norma OS/models
///   - 负断言：不得退回旧 assets/models/paraformer 路径
#[test]
fn test_model_installation_root_is_stable_under_application_data() {
    let root = crate::model_installation_root(std::path::Path::new("/tmp/Norma OS"));

    assert_eq!(
        root,
        std::path::PathBuf::from("/tmp/Norma OS/models"),
        "ASR 模型根必须固定在应用数据目录的 models 子目录"
    );
    assert_ne!(
        root,
        std::path::PathBuf::from("assets/models/paraformer"),
        "新架构不得退回旧仓库内 Paraformer 模型路径"
    );
}
use std::fs;

/// ASR-INSTALL-1：只有清单与必要文件都匹配时，Paraformer 才是可用模型。
///
/// 数据构造（含关键数值的推导过程）：
///   schema version      = 1，首版安装清单
///   required file count = 3，encoder + decoder + tokens
///   model file bytes    = 每个 1 byte，本测试只验证存在性而不加载 ONNX
///   removal count       = 1，删除 decoder 制造明确的不完整安装
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入与 Paraformer descriptor 匹配的安装清单与 3 个必要文件
///   2. validate_installation → 生成 AvailableAsrModel
///   3. 删除 decoder.int8.onnx 后再验证 → 返回缺失文件错误
///
/// 预期结果：
///   - 正断言：完整安装返回 Paraformer 身份与精确根目录
///   - 负断言：缺少 decoder 后不再是可用模型
#[test]
fn test_validate_installation_requires_matching_manifest_and_all_model_files() {
    let temp = tempfile::tempdir().expect("应可创建模型安装临时目录");
    let descriptor = builtin_model_catalog()
        .entries()
        .first()
        .expect("受控模型库应包含 Paraformer")
        .clone();
    for file in descriptor.artifact().required_files() {
        fs::write(temp.path().join(file), [1_u8]).expect("应可写入模型占位文件");
    }
    InstallationManifest::for_descriptor(&descriptor)
        .write_to(temp.path())
        .expect("应可写入安装清单");

    let available = validate_installation(temp.path(), &descriptor)
        .expect("清单与三个必要文件完整时应是可用模型");

    assert_eq!(
        available.descriptor().id().as_str(),
        PARAFORMER_MODEL_ID,
        "可用模型必须保留受控模型库中的 Paraformer 身份"
    );
    assert_eq!(
        available.root(),
        temp.path(),
        "可用模型必须指向已校验的精确安装根目录"
    );

    fs::remove_file(temp.path().join("decoder.int8.onnx"))
        .expect("应可删除 decoder 以模拟安装损坏");
    let broken = validate_installation(temp.path(), &descriptor);

    assert!(
        broken.is_err(),
        "缺少 decoder 的 Paraformer 不应继续被识别为 Ready"
    );
    assert!(
        broken
            .expect_err("缺少 decoder 必须返回错误")
            .to_string()
            .contains("decoder.int8.onnx"),
        "安装错误应指出精确缺失的 decoder 文件"
    );
}

/// ASR-INSTALL-2：当前模型禁止删除，取消选择后才能完整移除安装。
///
/// 数据构造（含关键数值的推导过程）：
///   ready installation = 1 个 Paraformer 目录
///   selected model     = 同一 Paraformer ID，因此删除必须被拒绝
///   unselected state   = None，表示用户已切换或清除选择
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造完整 Paraformer 安装与清单
///   2. 传入 selected Paraformer 删除 → 拒绝且目录保留
///   3. 传入 None 再删除 → 安装目录整体移除
///
/// 预期结果：
///   - 正断言：未选中的安装可被删除
///   - 负断言：当前模型的清单和文件不得被删除
#[test]
fn test_remove_installation_protects_selected_model_then_removes_unselected_model() {
    let temp = tempfile::tempdir().expect("应可创建删除测试临时目录");
    let descriptor = builtin_model_catalog()
        .entries()
        .first()
        .expect("受控模型库应包含 Paraformer")
        .clone();
    for file in descriptor.artifact().required_files() {
        fs::write(temp.path().join(file), [1_u8]).expect("应可写入模型占位文件");
    }
    InstallationManifest::for_descriptor(&descriptor)
        .write_to(temp.path())
        .expect("应可写入安装清单");
    let selected = AsrModelId::parse(PARAFORMER_MODEL_ID).expect("内置 Paraformer ID 应有效");

    let protected = remove_installation(temp.path(), &descriptor, Some(&selected));

    assert!(protected.is_err(), "当前选中的 Paraformer 必须拒绝删除");
    assert!(
        temp.path().join("encoder.int8.onnx").is_file(),
        "删除保护失败时 encoder 必须保留"
    );
    assert!(
        temp.path().join(INSTALLATION_MANIFEST_FILE).is_file(),
        "删除保护失败时安装清单必须保留"
    );

    remove_installation(temp.path(), &descriptor, None)
        .expect("取消选择后应可删除 Paraformer 安装");

    assert!(
        !temp.path().exists(),
        "未选中的模型删除后安装根目录不应残留"
    );
}

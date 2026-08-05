use canvas::{PreviewSpec, PreviewType};

/// 旧版单图 Preview 快照反序列化后仍保留唯一主路径，且不会凭空产生附加图片。
///
/// 数据构造（含关键数值的推导过程）：
///   legacy JSON 字段数 = 2（path + explicit_type）
///   主路径数             = 1
///   旧快照附加路径数     = 0（旧 JSON 中不存在 additional_paths）
///   all_paths 总数       = 1 + 0 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 反序列化不含 additional_paths 的旧版 JSON。
///   2. 读取主路径、附加路径与按浏览顺序展开的 all_paths。
///   3. 检查类型提示仍为 Image。
///
/// 预期结果：
///   - 正断言：主路径和 Image 类型完整保留，all_paths 只有旧主路径。
///   - 负断言：附加路径必须为空，不得因兼容逻辑生成重复图片。
#[test]
fn legacy_single_image_preview_spec_deserializes_without_duplicate_paths() {
    let legacy_json = r#"{"path":"/tmp/cover.png","explicit_type":"Image"}"#;

    let spec: PreviewSpec =
        serde_json::from_str(legacy_json).expect("旧版单图 PreviewSpec 应继续可反序列化");
    let all_paths = spec.all_paths().collect::<Vec<_>>();

    assert_eq!(
        spec.path, "/tmp/cover.png",
        "旧快照的主图片路径必须原样保留"
    );
    assert_eq!(
        spec.explicit_type,
        Some(PreviewType::Image),
        "旧快照的显式 Image 类型必须原样保留"
    );
    assert_eq!(
        all_paths,
        vec!["/tmp/cover.png"],
        "旧单图快照展开后应恰好只有一张图片"
    );
    assert!(
        spec.additional_paths.is_empty(),
        "旧快照没有附加路径，兼容反序列化不得生成重复图片"
    );
}

use crate::providers::doubao::{default_voice, find_voice, voices};
use std::collections::HashSet;

/// 官方 2.0 音色目录完整且可寻址，不混入 1.0 或 S2S 专用音色。
///
/// 数据构造（含关键数值的推导过程）：
///   2026-10-04 官方文档的两个 2.0 表格：294 个通用/角色音色 + 137 个外语音色 = 431。
///   独立对照选取默认 Vivi、中文云舟、外语 Dina 和官方收录的角色 Charlie。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取随应用发布的目录 → 比较文档数量并检查重复 ID。
///   2. 按稳定 ID 查询中文和外语音色 → 检查官方名称。
///   3. 查询旧 1.0 和未知 ID → 不应找到兼容音色。
///
/// 预期结果：431 个唯一音色均可加载，具体示例名称正确；旧资源和未知 ID 被拒绝。
#[test]
fn doubao_catalog_contains_official_tts_2_voices() {
    assert_eq!(
        voices().len(),
        431,
        "2.0 目录必须包含官方两个表格的 431 个音色"
    );
    let ids: HashSet<_> = voices().iter().map(|voice| &voice.id).collect();
    assert_eq!(
        ids.len(),
        431,
        "音色 ID 不能重复，否则选择与列表行身份可能冲突"
    );
    assert_eq!(
        default_voice().name,
        "Vivi 2.0",
        "升级默认必须保留原 Vivi 音色"
    );
    assert_eq!(
        find_voice("zh_male_m191_uranus_bigtts")
            .expect("云舟应存在")
            .name,
        "云舟 2.0",
        "云舟 ID 必须匹配官方名称"
    );
    assert_eq!(
        find_voice("ar_female_dina_uranus_bigtts")
            .expect("Dina 应存在")
            .name,
        "Dina",
        "必须支持官方外语表中的音色"
    );
    assert_eq!(
        find_voice("ICL_uranus_en_female_charlie_tob")
            .expect("Charlie 应存在")
            .name,
        "Charlie 2.0",
        "官方系统音色不能仅因 ICL 前缀而丢失"
    );
    assert!(
        find_voice("zh_female_shuangkuaisisi_moon_bigtts").is_none(),
        "1.0 音色不能进入固定 2.0 目录"
    );
    assert!(
        find_voice("private-cloned-voice").is_none(),
        "私有克隆音色不能冒充官方系统音色"
    );
}

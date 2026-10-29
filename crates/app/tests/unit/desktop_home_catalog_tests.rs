use super::*;

fn entry(id: &str, days: i64, running: bool) -> HomeEntry {
    HomeEntry {
        id: id.into(),
        name: id.into(),
        project_count: 1,
        opened_ms: (30 - days) * 86_400_000,
        running,
    }
}

/// 场景：最近访问按时间倒序，搜索分组运行优先且更早可折叠。
/// 数据：now=30天；1个运行、1个恰好7天、4个8天以上；7天=604800000ms。
/// 执行：取最近四个，读取折叠/展开目录，再搜索旧空间。
/// 预期：运行空间不重复归类；7天边界属于最近；4个更早只露2个；搜索不受折叠影响。
#[test]
fn desktop_home_catalog_recency_groups_and_collapse() {
    let data = vec![
        entry("old4", 20, false),
        entry("running", 29, true),
        entry("week", 7, false),
        entry("old1", 8, false),
        entry("old2", 9, false),
        entry("old3", 10, false),
    ];
    let recent = recent(&data);
    assert_eq!(
        recent.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["week", "old1", "old2", "old3"],
        "最近卡片按访问时间排序，不按创建顺序"
    );
    let grouped = groups(&data, "", false, 30 * 86_400_000);
    assert_eq!(grouped.len(), 3, "显示三个非空分组");
    assert_eq!(grouped[0].kind, GroupKind::Running, "运行中优先");
    assert_eq!(grouped[1].entries[0].id, "week", "恰好7天仍属于最近7天");
    assert_eq!(grouped[2].total, 4, "更早显示完整数量");
    assert_eq!(grouped[2].entries.len(), 2, "更早超过3个仅露2个");
    assert_eq!(
        groups(&data, "", true, 30 * 86_400_000)[2].entries.len(),
        4,
        "展开后全部可达"
    );
    let found = groups(&data, "OLD4", false, 30 * 86_400_000);
    assert_eq!(found[0].entries[0].id, "old4", "查询忽略大小写且包含折叠项");
    assert_eq!(found[0].kind, GroupKind::Matches, "查询统一显示结果");
    assert!(
        groups(&data, "missing", false, 0).is_empty(),
        "无结果不能保留旧选择"
    );
}

/// 场景：Unicode匹配高亮保持原字符串的合法UTF-8边界。
/// 数据：İ的小写为i+组合点，前缀emoji占4字节，中文每字3字节。
/// 执行：对大小写扩展与中文查询取原串切片，再测空查询。
/// 预期：高亮原字符而非小写串偏移；空串不产生高亮。
#[test]
fn desktop_home_unicode_highlights_preserve_original_boundaries() {
    let name = "🪐İOS 设计设计";
    let hits = highlight_ranges(name, "i");
    assert_eq!(
        &name[hits[0].clone()],
        "İ",
        "大小写扩展必须映射回完整原字符"
    );
    let hits = highlight_ranges(name, "设计");
    assert_eq!(hits.len(), 2, "所有匹配都应高亮");
    assert_eq!(&name[hits[1].clone()], "设计", "中文切片不能截断UTF-8");
    assert!(highlight_ranges(name, " ").is_empty(), "空查询不高亮整串");
}

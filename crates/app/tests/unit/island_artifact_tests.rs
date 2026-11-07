use super::*;
use norma::mobile::{RunOverviewAttention, RunOverviewAttentionKind};

/// 场景：从岛给后台工作空间的成果标记已看，不能清除其他空间或其他版本。
/// 数据：B/a/7的v1有alpha、beta两个文件，A同名成果、B的v2仍未读。
/// 执行：先保存B的单文件回执，再保存整个回合；通过SettingsStore读取生产键。
/// 预期：先只读alpha，再读beta；旧格式与原有内容预览回执兼容，不串空间/版本。
#[test]
fn island_artifact_receipts_are_scoped_and_retain_unrelated_versions() {
    let temp = tempfile::tempdir().expect("隔离回执目录");
    let settings = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut row = RunOverviewAttention {
        kind: RunOverviewAttentionKind::File,
        area_id: "a".into(),
        node_id: 7,
        project: "project".into(),
        agent: "Iris".into(),
        title: "Result".into(),
        run_id: "r".into(),
        turn_id: "t".into(),
        request_id: None,
        fingerprint: Some("v1".into()),
        paths: vec!["alpha.txt".into()],
        loaded: false,
    };
    save_artifact_receipts(&settings, "B", &row).expect("后台成果单文件已读");
    let key = |workspace, version, path| {
        format!("desktop.artifact.read.v1:[\"{workspace}\",\"a\",7,\"{version}\",\"{path}\"]")
    };
    assert_eq!(
        settings
            .get_string(&key("B", "v1", "alpha.txt"))
            .unwrap()
            .as_deref(),
        Some("true"),
        "原内容预览必须识别岛写入的回执"
    );
    assert_eq!(
        settings.get_string(&key("B", "v1", "beta.txt")).unwrap(),
        None,
        "单文件不应清除同回合其他文件"
    );
    row.kind = RunOverviewAttentionKind::Review;
    row.paths.push("beta.txt".into());
    save_artifact_receipts(&settings, "B", &row).expect("整个回合已读");
    assert_eq!(
        settings
            .get_string(&key("B", "v1", "beta.txt"))
            .unwrap()
            .as_deref(),
        Some("true"),
        "回合已读应覆盖所有文件"
    );
    assert_eq!(
        settings.get_string(&key("A", "v1", "alpha.txt")).unwrap(),
        None,
        "不能串工作空间"
    );
    assert_eq!(
        settings.get_string(&key("B", "v2", "alpha.txt")).unwrap(),
        None,
        "不能提前已读新版本"
    );
    row.kind = RunOverviewAttentionKind::Failed;
    assert!(
        save_artifact_receipts(&settings, "B", &row).is_err(),
        "错误行不允许伪装成成果已读"
    );
}

/// 场景：同一回合的两个文件快速各点一次已看，不应在异步保存中互相去重。
/// 数据：完全相同来源/空间/节点/版本的alpha、beta两个文件和整回合成果。
/// 执行：生成待处理操作身份，比较不同文件、整回合，以及同一文件的重复点击。
/// 预期：两个文件和整回合各自独立；同一文件的重复点击才共享身份。
#[test]
fn island_artifact_pending_identity_includes_the_selected_files() {
    let route = IslandTarget {
        source: 1,
        workspace: "B".into(),
        area: "a".into(),
    };
    let mut row = RunOverviewAttention {
        kind: RunOverviewAttentionKind::File,
        area_id: "a".into(),
        node_id: 7,
        project: "project".into(),
        agent: "Iris".into(),
        title: "Result".into(),
        run_id: "r".into(),
        turn_id: "t".into(),
        request_id: None,
        fingerprint: Some("v1".into()),
        paths: vec!["alpha.txt".into()],
        loaded: false,
    };
    let first = artifact_pending_id(&route, &row);
    assert_eq!(
        first,
        artifact_pending_id(&route, &row),
        "同一文件重复点击应去重"
    );
    row.paths = vec!["beta.txt".into()];
    assert_ne!(
        first,
        artifact_pending_id(&route, &row),
        "不能丢弃另一文件的点击"
    );
    row.kind = RunOverviewAttentionKind::Review;
    row.paths = vec!["alpha.txt".into(), "beta.txt".into()];
    assert_ne!(
        first,
        artifact_pending_id(&route, &row),
        "整个回合的已读范围独立"
    );
}

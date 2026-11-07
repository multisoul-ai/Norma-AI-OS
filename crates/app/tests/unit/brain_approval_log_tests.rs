use super::*;
use gpui::{AppContext, TestAppContext};

/// 审批审计必须写进全局日志，并携带准确来源，不能写在提议子目录。
/// 数据：同一个App内alpha/beta两个项目、同名审批编号、各自route，迁移后的真实目录。
/// 执行：调用生产审计落盘入口并等待后台写入屏障，然后读取全局日志。
/// 预期：两条记录都存在且身份分离；提议目录没有误放的日志。
#[test]
fn brain_approval_audit_uses_prepared_global_log_and_scoped_identity() {
    let dir = tempfile::tempdir().unwrap();
    let current = brain::migration::migrate(&dir.path().join("brain"), &[])
        .unwrap()
        .directory;
    let mut test = TestAppContext::single();
    test.update(|cx| {
        let owner = crate::app::brain_host::owner::shared(cx);
        assert!(
            owner.begin_prepare(std::time::Instant::now()),
            "准备唯一数据所有者"
        );
        owner.finish_prepare(Ok(crate::app::brain_host::data::Prepared {
            directory: current.clone(),
            settings: Default::default(),
        }));
    });
    for name in ["alpha", "beta"] {
        let app = test.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                storage::DatabasePaths::isolated_test(&dir.path().join(name)),
            )
        });
        app.update(&mut test, |app, cx| {
            app.workspace_id = Some(name.into());
            app.workspace_route_id = Some(format!("route-{name}"));
            app.brain.proposals_path = Some(brain::migration::proposal_file(&current, name));
            app.log_decision(
                "same-request",
                BrainDecision::Deny,
                "scoped audit",
                Some("command"),
                cx,
            );
        });
    }
    brain::persistence::shared_writer()
        .flush()
        .recv()
        .unwrap()
        .unwrap();
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(current.join("decisions.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2, "两条审计写进统一全局日志");
    for name in ["alpha", "beta"] {
        assert!(
            lines.iter().any(|line| line["workspace_id"] == name
                && line["route_id"] == format!("route-{name}")
                && line["approval"] == "same-request"),
            "{name}原始身份准确"
        );
    }
    assert!(
        !current.join("proposals/approvals.jsonl").exists(),
        "日志不能误写进提议目录"
    );
}

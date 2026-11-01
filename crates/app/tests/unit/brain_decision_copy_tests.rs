use super::*;

/// 场景：直接协议调用不能绕过 CLI 的长度校验而进入审批副作用。
/// 构造：隔离 App、有效幕僚令牌、未知审批身份及 51 字符理由。
/// 执行：经生产 execute_brain_command 入口提交 Allow。
/// 预期：优先返回带具体计数的文案错误，不能继续查找/处理审批。
#[test]
fn invalid_decision_copy_is_rejected_before_host_actions() {
    let dir = tempfile::tempdir().unwrap();
    let mut test = gpui::TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            storage::DatabasePaths::isolated_test(&dir.path().join("data")),
        )
    });
    source.update(cx, |app, cx| {
        let (_tx, status) = std::sync::mpsc::channel();
        app.brain.host = Some(BrainHost {
            service: None,
            status,
            token: "copy-test".into(),
        });
        let error = app
            .execute_brain_command(
                "copy-test",
                norma::protocol::BrainOp::Decide {
                    approval_id: "not-current".into(),
                    decision: norma::protocol::BrainDecision::Allow,
                    reason: "字".repeat(51),
                },
                None,
                cx,
            )
            .unwrap_err();
        let message = match error {
            norma::protocol::CardCommandError::Agent(error) => error.message,
            other => panic!("文案错误应有结构化命令错误：{other:?}"),
        };
        assert!(
            message.contains("decision_copy_"),
            "宿主必须校验文案：{message}"
        );
        assert!(
            message.contains("actual=51"),
            "错误应告知实际计数：{message}"
        );
        assert!(
            !message.contains("approval_closed"),
            "不能先走审批副作用：{message}"
        );
        assert!(app.brain.tracked.is_empty(), "无效文案不能生成或替换审批");
    });
}

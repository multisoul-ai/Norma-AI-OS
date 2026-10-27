use super::*;
use norma::protocol::BrainDecision;

/// 场景：宿主已接收 Allow 或 Deny，需要保存可供报告引用的决定，不能推断命令执行完成。
/// 构造：隔离库与宿主成功回执夹具；覆盖合法长审批身份和空白理由，不调用真实供应商。
/// 执行：通过宿主来源保存 Interface，随后精确读回两条引用。
/// 预期：Allowed/Denied 区分明确，实际回执为 Accepted，长身份可追溯；此测试不替代 Runtime 审批验收。
#[tokio::test]
async fn brain_action_context_decisions_preserve_status_and_identity() {
    for (choice, expected) in [
        (BrainDecision::Allow, DecisionStatus::Allowed),
        (BrainDecision::Deny, DecisionStatus::Denied),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let service = SharedContext::new(
            dir.path().join("context.sqlite"),
            ContextScope {
                workspace_id: "w".into(),
                area_id: "a".into(),
                project: ContextProject::Personal,
                task_id: None,
                conversation_id: None,
            },
        )
        .unwrap();
        let approval_id = format!(
            "7.Codex.{}.{}.{}.{}",
            "r".repeat(128),
            "s".repeat(128),
            "t".repeat(128),
            "q".repeat(256)
        );
        let mut result = CardCommandResult::Brain {
            output: json!({"decision":format!("{choice:?}")}).to_string(),
        };
        service
            .save_brain_action_result(
                BrainOp::Decide {
                    approval_id: approval_id.clone(),
                    decision: choice,
                    reason: " \n".into(),
                },
                &mut result,
            )
            .await;
        let CardCommandResult::Brain { output } = result else {
            panic!("应返回幕僚回执")
        };
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            value["context"]["saved"], true,
            "已接受的审批选择需要可追溯来源"
        );
        let sources: Vec<ContextRef> =
            serde_json::from_value(value["context"]["sources"].clone()).unwrap();
        assert_eq!(sources.len(), 2, "决定和接收回执应各一条");
        let records = service
            .run(None, move |access| {
                sources
                    .iter()
                    .map(|r| access.read(r))
                    .collect::<storage::StorageResult<Vec<_>>>()
            })
            .await
            .unwrap();
        assert_eq!(
            records[0].event.payload,
            ContextPayload::Decision {
                status: expected,
                reason: "未提供理由".into()
            },
            "选择与空白理由必须如实记录"
        );
        let ContextPayload::Receipt {
            status,
            operation_id,
            summary,
        } = &records[1].event.payload
        else {
            panic!("必须返回真实选择回执")
        };
        assert_eq!(
            *status,
            ReceiptStatus::Accepted,
            "Runtime 接收选择不等于被请求命令已经执行成功"
        );
        assert!(
            operation_id.len() <= 256 && !operation_id.is_empty(),
            "长审批应映射到有界身份"
        );
        assert!(summary.contains(&approval_id), "仍应能追溯完整原审批身份");
        assert_eq!(
            records[1].event.related,
            vec![records[0].reference.clone()],
            "回执不能串到其他决定"
        );
        assert!(
            service.snapshot().await.unwrap().state.reports.is_empty(),
            "保存来源不能自行生成播报或重放动作"
        );
    }
}

/// 场景：实际动作已经接收，来源保存遇到同范围队列阻塞，CLI 等待者超时或被释放。
/// 构造：已接受的 Deny 回执夹具，通过生产 run Interface 的后台任务持有串行队列。
/// 执行：启动保存后分别等两秒超时或只 poll 一次就释放等待者；解除阻塞并换新消费者读取。
/// 预期：超时不称已保存，宿主仍最终保存恰好一个决定/回执；测试只验证保存生命周期，不执行审批。
#[tokio::test]
async fn brain_action_context_save_outlives_timeout_and_waiter() {
    for abandon in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("context.sqlite");
        let scope = ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        };
        let service = SharedContext::new(path.clone(), scope.clone()).unwrap();
        service.snapshot().await.unwrap();
        let (started, ready) = crossbeam_channel::bounded(1);
        let (release, held) = crossbeam_channel::bounded(1);
        let blocker = service.clone();
        let blocking = tokio::spawn(async move {
            blocker
                .run(None, move |_| {
                    started.send(()).map_err(|_| {
                        storage::StorageError::InvalidContext("fixture_start_closed".into())
                    })?;
                    held.recv().map_err(|_| {
                        storage::StorageError::InvalidContext("fixture_released".into())
                    })
                })
                .await
        });
        tokio::task::spawn_blocking(move || ready.recv_timeout(std::time::Duration::from_secs(5)))
            .await
            .unwrap()
            .unwrap();
        let mut result = CardCommandResult::Brain {
            output: json!({"decision":"Deny"}).to_string(),
        };
        {
            let mut work = Box::pin(service.save_brain_action_result(
                BrainOp::Decide {
                    approval_id: "7.Codex.r.s.t.q".into(),
                    decision: BrainDecision::Deny,
                    reason: "待确认范围".into(),
                },
                &mut result,
            ));
            if abandon {
                assert!(
                    futures::poll!(work.as_mut()).is_pending(),
                    "保存必须先实际启动，再释放等待者"
                );
            } else {
                let saved = tokio::time::timeout(std::time::Duration::from_secs(5), work)
                    .await
                    .expect("保存等待必须在测试上限内结束，不能被串行队列永久阻塞");
                assert!(!saved, "超过两秒等待上限不能声称已保存");
            }
        }
        if !abandon {
            let CardCommandResult::Brain { output } = &result else {
                unreachable!()
            };
            let value: serde_json::Value = serde_json::from_str(output).unwrap();
            assert_eq!(value["context"]["saved"], false, "等待超时不能当作提交确认");
            assert!(
                value["context"]["error"]
                    .as_str()
                    .is_some_and(|error| error.ends_with("context_save_pending")),
                "应明确表示仍待确认保存"
            );
            assert_eq!(
                value["context"]["retry_action"], false,
                "等待超时也不能重做实际审批"
            );
        }
        release.send(()).unwrap();
        drop(service);
        blocking.await.unwrap().unwrap();
        let reloaded = SharedContext::new(path, scope).unwrap();
        let snapshot = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let snapshot = reloaded.snapshot().await.unwrap();
                if snapshot.events.len() == 2 {
                    break snapshot;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("原等待者退出后，宿主仍须保存已经接收的证据");
        assert_eq!(
            snapshot
                .events
                .iter()
                .filter(|r| matches!(
                    r.event.payload,
                    ContextPayload::Decision {
                        status: DecisionStatus::Denied,
                        ..
                    }
                ))
                .count(),
            1,
            "决定只能提交一次"
        );
        let receipt = snapshot
            .events
            .iter()
            .find(|r| {
                matches!(
                    r.event.payload,
                    ContextPayload::Receipt {
                        status: ReceiptStatus::Accepted,
                        ..
                    }
                )
            })
            .expect("应保留接收回执");
        assert_eq!(receipt.event.related.len(), 1, "回执仍关联唯一决定");
    }
}

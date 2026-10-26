use super::*;
use crate::app::shared_context::SharedContext;
use agent::context::{ContextPayload, ContextProject, ContextScope};

async fn fixture(text: &str) -> (tempfile::TempDir, SharedContext, ReportDeliveryHandle) {
    let dir = tempfile::tempdir().unwrap();
    let service = SharedContext::new(
        dir.path().join("context.sqlite"),
        ContextScope {
            workspace_id: "workspace".into(),
            area_id: "a".into(),
            project: ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
    )
    .unwrap();
    let text = text.to_owned();
    let record = service
        .run(None, move |access| access.report("native", &text, vec![]))
        .await
        .unwrap();
    let delivery = service.report_delivery(record.reference).unwrap();
    delivery.set(ReportDelivery::Queued);
    (dir, service, delivery)
}

async fn status(service: &SharedContext, delivery: &ReportDeliveryHandle) -> ReportDelivery {
    delivery.flush().await.unwrap();
    service.snapshot().await.unwrap().state.reports[&delivery.reference().event_id].delivery
}

/// 场景：长中文和需转义的报告要完整追加，所有资料 ACK 前不能提交播报请求。
/// 构造：中英混合及引号、反斜杠和换行；真实共享记录和原生分段队列。
/// 执行：发送所有分段，扣住最后 ACK，注入同 ID 错类型 ACK，再补正确 ACK 和最终 ACK。
/// 预期：每段含包裹 ≤500 字节，重组正文相同；缺段不发 Commentary；接收可证而播放保持 Unknown。
#[tokio::test]
async fn gpt_live_native_report_waits_for_all_typed_acks_and_never_claims_playback() {
    let text = "中\"\\\n".repeat(300);
    let (_dir, service, delivery) = fixture(&text).await;
    let mut queue = NativeReports::default();
    queue.push(NativeReport::new(
        text.clone(),
        ReportPriority::Normal,
        delivery.clone(),
        CancellationToken::new(),
    ));
    let count = queue.0[0].fragments.len();
    assert!(count > 1 && count <= 64, "长报告必须产生有界多段");
    let mut ids = Vec::new();
    let mut assembled = String::new();
    for index in 0..count {
        queue
            .pump(|command, _scope| {
                let LiveCommand::Append {
                    event_id,
                    kind,
                    content,
                    delegation_id,
                } = command
                else {
                    panic!("应为追加")
                };
                assert_eq!(*kind, AppendKind::Thinking, "资料未齐时只追加资料");
                assert!(delegation_id.is_none(), "幕僚报告不能冒用用户委派");
                assert!(content.len() <= 500, "单条内容含元数据和转义不得超限");
                let data: serde_json::Value = serde_json::from_str(content).unwrap();
                assert_eq!(
                    data["event_id"],
                    delivery.reference().event_id,
                    "每段绑定真实报告引用"
                );
                assert_eq!(
                    data["revision"],
                    delivery.reference().revision,
                    "每段携带原报告版本"
                );
                assert_eq!(data["part"], index + 1, "顺序完整");
                assert_eq!(data["total"], count, "总段数明确");
                assembled.push_str(data["text"].as_str().unwrap());
                ids.push(event_id.clone());
                Ok(())
            })
            .unwrap();
        queue
            .pump(|_, _| panic!("每段收到 ACK 前不能继续投递"))
            .unwrap();
        if index + 1 < count {
            queue.acknowledge(&ids[index], AppendKind::Thinking);
        }
    }
    assert_eq!(assembled, text, "不能按字符截断丢失报告正文");
    queue.acknowledge(&ids[count - 1], AppendKind::Instructions);
    queue
        .pump(|_, _| panic!("错误类型 ACK 不能释放 Commentary"))
        .unwrap();
    assert_eq!(
        status(&service, &delivery).await,
        ReportDelivery::Queued,
        "局部 ACK 不冒充整份报告已接收"
    );
    queue.acknowledge(&ids[count - 1], AppendKind::Thinking);
    let mut final_id = String::new();
    queue
        .pump(|command, _scope| {
            let LiveCommand::Append {
                event_id,
                kind,
                content,
                ..
            } = command
            else {
                panic!("应为最终追加")
            };
            assert_eq!(*kind, AppendKind::Commentary, "收齐资料后才允许请求汇报");
            let data: serde_json::Value = serde_json::from_str(content).unwrap();
            assert_eq!(data["kind"], "brain_report_ready", "最终标记不伪造用户发言");
            final_id = event_id.clone();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        status(&service, &delivery).await,
        ReportDelivery::Queued,
        "本地控制队列接受不冒充网络发送"
    );
    queue.acknowledge(&final_id, AppendKind::Commentary);
    assert!(queue.0.is_empty(), "完整接收后释放在途资源");
    assert_eq!(
        status(&service, &delivery).await,
        ReportDelivery::Unknown,
        "无音频归属证据不能声称已播"
    );
    let recorded: Vec<_> = service
        .snapshot()
        .await
        .unwrap()
        .events
        .iter()
        .filter_map(|record| {
            if let ContextPayload::Delivery { status, .. } = record.event.payload {
                Some(status)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        recorded,
        vec![
            ReportDelivery::Queued,
            ReportDelivery::Sent,
            ReportDelivery::Acknowledged,
            ReportDelivery::Unknown
        ],
        "保留完整传输证据而不伪造 Playing / Completed"
    );
}

/// 场景：满队列只能重试未接收命令；取消或超时后不能补发已接收片段。
/// 构造：真实报告、一次 Capacity 拒绝、随后一次成功接收；分别取消或推进到期。
/// 执行：比较两次提交 ID，随后再泵队列，最后注入迟到 ACK。
/// 预期：仅未接收时重试同一 ID；取消后零新命令，部分发送记 Unknown，迟到 ACK 不复活。
#[tokio::test]
async fn gpt_live_native_report_backpressure_cancellation_and_deadline_do_not_replay() {
    for cancel in [true, false] {
        let (_dir, service, delivery) = fixture("待确认报告").await;
        let token = CancellationToken::new();
        let mut queue = NativeReports::default();
        queue.push(NativeReport::new(
            "待确认报告".into(),
            ReportPriority::Normal,
            delivery.clone(),
            token.clone(),
        ));
        let mut attempted = Vec::new();
        for accepted in [false, true] {
            queue
                .pump(|command, _scope| {
                    let LiveCommand::Append { event_id, .. } = command else {
                        panic!("应为追加")
                    };
                    attempted.push(event_id.clone());
                    if accepted {
                        Ok(())
                    } else {
                        Err(LiveError::Capacity)
                    }
                })
                .unwrap();
        }
        assert_eq!(attempted.len(), 2, "一次拒绝后只重新申请一次接收");
        assert_eq!(attempted[0], attempted[1], "不能制造第二个追加身份");
        if cancel {
            token.cancel();
        } else {
            queue.0[0].expires = std::time::Instant::now();
        }
        queue.pump(|_, _| panic!("取消或到期后不能补发")).unwrap();
        assert!(queue.0.is_empty(), "必须释放过期队列");
        queue.acknowledge(&attempted[0], AppendKind::Thinking);
        assert_eq!(
            status(&service, &delivery).await,
            ReportDelivery::Unknown,
            "部分发送无法认定未播，也不能认定完成"
        );
    }
    let (_dir, service, delivery) = fixture("未发送").await;
    let token = CancellationToken::new();
    token.cancel();
    let mut queue = NativeReports::default();
    queue.push(NativeReport::new(
        "未发送".into(),
        ReportPriority::Normal,
        delivery.clone(),
        token,
    ));
    assert!(queue.0.is_empty(), "已取消报告不能占队");
    assert_eq!(
        status(&service, &delivery).await,
        ReportDelivery::Skipped,
        "未发送取消才可认定跳过"
    );
}

/// 场景：前台恰好在队列筛选后、传输接收前关闭幕僚播报。
/// 构造：分别准备尚未发送和已有一段被接收的报告，发送闭包模拟这个精确取消窗口。
/// 执行：闭包取消本报告令牌并返回 Cancelled，再检查泵结果、队列和真实账本。
/// 预期：仅撤销报告，主音频循环不收到错误；未发记 Skipped，部分接收记 Unknown。
#[tokio::test]
async fn gpt_live_native_report_cancel_during_admission_does_not_end_call() {
    for partial in [false, true] {
        let (_dir, service, delivery) = fixture("分段报告").await;
        let mut queue = NativeReports::default();
        queue.push(NativeReport::new(
            "分段报告".into(),
            ReportPriority::Normal,
            delivery.clone(),
            CancellationToken::new(),
        ));
        if partial {
            let mut id = String::new();
            queue
                .pump(|command, _| {
                    if let LiveCommand::Append { event_id, .. } = command {
                        id = event_id.clone();
                    }
                    Ok(())
                })
                .unwrap();
            queue.acknowledge(&id, AppendKind::Thinking);
        }
        let result = queue.pump(|_, scope| {
            scope.cancel();
            Err(LiveError::Cancelled)
        });
        assert!(result.is_ok(), "局部报告取消不能传播为通话错误：{result:?}");
        assert!(queue.0.is_empty(), "被取消报告应立即释放");
        assert_eq!(
            status(&service, &delivery).await,
            if partial {
                ReportDelivery::Unknown
            } else {
                ReportDelivery::Skipped
            },
            "交付状态依据是否已向传输队列提交"
        );
    }
}

use super::*;

/// 场景：OpenAI拒绝工具定义时不能向用户谎报网络或Key权限故障。
/// 构造：使用真实日志中的稳定错误码invalid_function_parameters，另放一个普通网络错误。
/// 执行：由实际NormaSessionError经GPT专属分类器生成用户反馈。
/// 预期：Schema错误明确为工具定义问题且未执行操作；普通运行时错误仍保留原提示。
#[test]
fn gpt_live_report_distinguishes_tool_schema_rejection_from_connectivity() {
    let failure = PlannerFailure::from(agent::NormaSessionError::Runtime(
        r#"HTTP 400: {"error":{"code":"invalid_function_parameters","message":"Invalid schema for function 'delete_canvas_card'"}}"#.into(),
    ));
    let message = DelegatedReport {
        planner: Err(failure),
        ..Default::default()
    }
    .message();
    assert!(
        message.contains("工具定义"),
        "必须准确说明工具定义被拒绝：{message}"
    );
    assert!(
        message.contains("未执行任何操作"),
        "请求被预检拒绝不能宣称已创建卡片"
    );
    assert!(
        !message.contains("网络") && !message.contains("权限"),
        "不能将代码问题归咎于用户网络或模型权限"
    );
    let network = DelegatedReport {
        planner: Err(PlannerFailure::from(agent::NormaSessionError::Runtime(
            "connection refused".into(),
        ))),
        ..Default::default()
    }
    .message();
    assert!(
        !network.contains("工具定义"),
        "普通网络错误不得误分类为Schema问题"
    );
}

/// AC05/AC10：项目、后台配置及实际VAD任一缺失时应及时结束委派并说明原因。
/// 构造：分别缺项目、后台和VAD，以及全部已就绪的四种组合。
/// 执行：调用生产准入检查，不模拟服务端转写完成事件。
/// 预期：缺项消息具体、就绪时放行；VAD只控制更正保护能力，不被当作转写水位。
#[test]
fn gpt_live_delegation_admission_reports_missing_dependencies() {
    assert!(
        admission(None, true, true)
            .expect("无项目应拒绝")
            .contains("打开项目"),
        "无项目不应无限等待"
    );
    assert!(
        admission(Some("/project"), false, true)
            .expect("无后台应拒绝")
            .contains("GPT-Live API Key"),
        "后台就绪必须约束实际派工"
    );
    assert!(
        admission(Some("/project"), true, false)
            .expect("VAD不可用应拒绝")
            .contains("语音检测"),
        "实际VAD失败应明确说明"
    );
    assert_eq!(
        admission(Some("/project"), true, true),
        None,
        "所有依赖就绪才允许派工"
    );
}

/// AC10/AC12：后台配置、模型超时与澄清必须回传，不能全部变成泛化的“未执行”。
/// 构造：缺配置、超时、正常澄清和谎称成功但没有工具回执的后台文字。
/// 执行：调用生产报告格式化入口，保持回执列表为空。
/// 预期：错误类别可区分；澄清保留，且任何文字草案都明确没有操作事实。
#[test]
fn gpt_live_report_preserves_failure_and_clarification_without_fabricating_effects() {
    let missing = DelegatedReport {
        planner: Err(PlannerFailure::Configuration),
        ..Default::default()
    }
    .message();
    let timeout = DelegatedReport {
        planner: Err(PlannerFailure::Timeout),
        ..Default::default()
    }
    .message();
    assert!(
        missing.contains("GPT-Live API Key"),
        "缺配置要给出具体可处理原因"
    );
    assert!(timeout.contains("超时"), "超时不能被误报为配置错误或成功");
    for text in ["请说明要打开哪个项目？", "已经创建了终端"] {
        let message = DelegatedReport {
            planner: Ok(Some(text.into())),
            ..Default::default()
        }
        .message();
        assert!(message.contains(text), "保留后台澄清草案");
        assert!(
            message.contains("未执行任何操作") && message.contains("不是执行回执"),
            "没有真实工具回执时，后台文字必须明确不能作执行事实：{message}"
        );
    }
}

/// AC10/AC12：只读任务和记忆查询需返回实际正文，长正文不能挤掉状态或超出注入预算。
/// 构造：含稳定任务ID/终态的真实AdapterResult形状，以及带中文正文的记忆输出。
/// 执行：经生产报告入口格式化两种回执，再加入超过预算的正文。
/// 预期：状态、ID、正文均可见；UTF-8有效且总量不超过480字节。
#[test]
fn gpt_live_report_includes_bounded_query_results() {
    let tasks = agent::AdapterResult::succeeded("查询成功").with_tasks(vec![
        serde_json::json!({"task_id":42,"state":"succeeded","agent":"Simon"}),
    ]);
    let message = DelegatedReport {
        receipts: vec![Ok(tasks)],
        ..Default::default()
    }
    .message();
    assert!(
        message.contains("Simon") && message.contains("42") && message.contains("succeeded"),
        "查询必须含真实任务事实：{message}"
    );
    let mut memory = agent::AdapterResult::succeeded("读取成功");
    memory.memory = Some(agent::MemoryToolOutput {
        markdown: Some(format!("默认中文回复{}", "很长的正文".repeat(100))),
        ..Default::default()
    });
    let message = DelegatedReport {
        receipts: vec![Ok(memory)],
        ..Default::default()
    }
    .message();
    assert!(message.contains("默认中文回复"), "记忆正文必须回传");
    assert!(
        message.len() <= 480,
        "Live 注入必须有界，实际{}字节",
        message.len()
    );
}

/// AC07/AC10：副作用线程异常退出而无回执时不能声称未执行；长任务正文不能遮蔽身份。
/// 构造：effects=true且receipts为空；任务查询含200次中文正文和真实task_id=42。
/// 执行：格式化两种生产完成快照。
/// 预期：第一种明确待核对；第二种先返回ID、状态和Agent，再限长正文。
#[test]
fn gpt_live_report_preserves_uncertainty_and_prioritizes_query_identity() {
    let unknown = DelegatedReport {
        effects: true,
        ..Default::default()
    }
    .message();
    assert!(unknown.contains("待核对"), "可能副作用必须保留不确定性");
    assert!(
        !unknown.contains("未执行任何操作"),
        "不可把缺回执当作无副作用"
    );
    let result = agent::AdapterResult::succeeded("查询成功").with_tasks(vec![serde_json::json!({"agent":"Simon","state":"succeeded","task":"长正文".repeat(200),"task_id":42})]);
    let message = DelegatedReport {
        receipts: vec![Ok(result)],
        ..Default::default()
    }
    .message();
    assert!(
        message.contains("42") && message.contains("Simon") && message.contains("succeeded"),
        "长正文前应保留真实身份和状态：{message}"
    );
}

/// AC10：控制通道满时真实结果必须保留，不能消费请求后静默丢弃。
/// 构造：容量1的通道先放普通指令，再把最终完成报告加入生产重试队列。
/// 执行：首次flush遇满→读取旧指令→再次flush→核对真实完成消息。
/// 预期：背压不丢报告，成功投递后队列才清空；无须重新执行任何工具。
#[test]
fn gpt_live_final_reports_survive_control_backpressure() {
    let (sender, receiver) = async_channel::bounded(1);
    sender
        .try_send(super::super::session::CallCommand::SetMuted(false))
        .expect("填满通道");
    let mut reports = FinalReports::default();
    reports
        .push("d1".into(), "真实task=42，accepted".into())
        .expect("保存最终报告");
    reports.flush(&sender).expect("遇满保留");
    assert!(!reports.is_empty(), "背压时不能清除最终报告");
    receiver.try_recv().expect("消费旧指令");
    reports.flush(&sender).expect("重新投递");
    assert!(reports.is_empty(), "成功进入控制队列后才完成本地提交");
    assert!(
        matches!(receiver.try_recv(),Ok(super::super::session::CallCommand::AppendResult {content,..}) if content.contains("42")),
        "最终报告原样到达"
    );
}

/// AC10：长只读正文不能遮蔽同一回合随后发生的真实创建事实。
/// 构造：超过480字节的记忆读取回执在前，node=9的真实创建回执在后。
/// 执行：通过生产报告入口格式化全部回执。
/// 预期：创建状态及node=9先于正文，整体仍有界且不丢失真实副作用身份。
#[test]
fn gpt_live_report_prioritizes_effects_over_long_read_bodies() {
    let mut read = agent::AdapterResult::succeeded("记忆已读取");
    read.memory = Some(agent::MemoryToolOutput {
        markdown: Some("长正文".repeat(200)),
        ..Default::default()
    });
    let created = agent::AdapterResult::succeeded("浏览器已创建").with_node_id(9);
    let message = DelegatedReport {
        receipts: vec![Ok(read), Ok(created)],
        effects: true,
        ..Default::default()
    }
    .message();
    assert!(
        message.contains("node=Some(9)"),
        "长查询正文不能遮蔽真实创建身份：{message}"
    );
    assert!(message.len() <= 480, "回传整体有界");
}

/// AC10：模型长草案或另一条记忆正文不能覆盖任务查询的真实身份。
/// 构造：task=42查询、480字节以上规划草案，以及位于查询前的长记忆读取。
/// 执行：格式化生产报告的两个组合。
/// 预期：任务ID/终态始终先于可选草案与正文，不能只剩顶层task=None。
#[test]
fn gpt_live_report_task_identity_precedes_planner_and_other_query_bodies() {
    for with_memory in [false, true] {
        let mut receipts = Vec::new();
        if with_memory {
            let mut read = agent::AdapterResult::succeeded("记忆已读取");
            read.memory = Some(agent::MemoryToolOutput {
                markdown: Some("长记忆".repeat(200)),
                ..Default::default()
            });
            receipts.push(Ok(read));
        }
        receipts.push(Ok(agent::AdapterResult::succeeded("任务已查询")
            .with_tasks(vec![
                serde_json::json!({"task_id":42,"state":"succeeded","agent":"Simon"}),
            ])));
        let message = DelegatedReport {
            receipts,
            planner: Ok(Some("长草案".repeat(200))),
            ..Default::default()
        }
        .message();
        assert!(
            message.contains("42") && message.contains("Simon") && message.contains("succeeded"),
            "实际查询身份优先于可选文本：{message}"
        );
    }
}

use agent::{realtime_command_authorized, AdapterCommand};

/// 实时工具以用户原话的明确执行意图为门槛；讨论和暂停播报不能被解释成派工/取消。
///
/// 数据构造：同一任务写操作、查询和 task_id=7 的取消；输入是原始转写，不使用工具自述。
/// 执行过程：明确修复 → 赞同想法 → 询问方案 → 否定执行 → 暂停汇报 → 明确取消 → 名词短语和无问号询问。
/// 预期结果：修复与明确取消分别可执行；查询可用；讨论、否定、播报控制和名词短语没有任务副作用。
#[test]
fn realtime_actions_require_execution_intent_from_the_user_turn() {
    let write = AdapterCommand::AssignAgentTask {
        agent: "codex".into(),
        task: "修复登录测试".into(),
    };
    let cancel = AdapterCommand::CancelAgentTask { task_id: 7 };
    let query = AdapterCommand::FindAgentTasks {
        task_id: None,
        agent: None,
        states: vec![],
    };
    assert!(
        realtime_command_authorized(&write, "请帮我修复登录测试，保留现有鉴权逻辑"),
        "明确执行原话应直接允许派工"
    );
    assert!(
        !realtime_command_authorized(&write, "这个思路不错"),
        "赞同想法不是执行授权"
    );
    assert!(
        !realtime_command_authorized(&write, "帮我分析一下要不要修复登录测试"),
        "讨论方案不能自动写入任务"
    );
    assert!(
        !realtime_command_authorized(&write, "请修复登录测试之前先解释原因，不要开始执行"),
        "后续否定必须阻止首句动词触发"
    );
    assert!(
        !realtime_command_authorized(&write, ""),
        "未收到原始转写不能信任模型工具请求"
    );
    assert!(
        !realtime_command_authorized(&cancel, "先别汇报"),
        "暂停播报不能取消后台任务"
    );
    assert!(
        !realtime_command_authorized(&cancel, "停止汇报"),
        "停止播报不包含取消任务意图"
    );
    assert!(
        !realtime_command_authorized(&write, "继续聊天"),
        "继续闲聊不能被解释成继续执行"
    );
    assert!(
        !realtime_command_authorized(&cancel, "请继续实现页面"),
        "执行授权不能冒充取消授权"
    );
    assert!(
        realtime_command_authorized(&cancel, "取消任务 7"),
        "明确取消应进入已有执行层确认"
    );
    assert!(
        realtime_command_authorized(&query, "这个思路不错"),
        "讨论仍可查询，不需要扩大权限"
    );
    assert!(
        !realtime_command_authorized(&write, "测试结果很好"),
        "测试结果的评价不是执行测试的祈使句"
    );
    assert!(
        !realtime_command_authorized(&write, "实现方案不错"),
        "实现方案的赞同不能因动词前缀变成派工"
    );
    assert!(
        !realtime_command_authorized(&cancel, "取消任务的按钮怎么设计"),
        "讨论取消按钮的设计不能取消实际任务"
    );
    assert!(
        !realtime_command_authorized(&cancel, "停止任务进度汇报"),
        "提到任务的汇报控制也不能取消任务"
    );
    assert!(
        !realtime_command_authorized(&write, "修复这个问题了吗"),
        "转写没有问号时仍应拒绝把进度询问当成写操作"
    );
}

/// 明确指定Agent的祈使句同样是派工；目标错配、名词评价和假设句仍不能执行。
///
/// 数据构造：目标profile=codex；输入复用真实.app L3中被错误拒绝的完整原话。
/// 执行过程：去掉礼貌前缀 → 匹配指定Agent → 对后续动词执行原有意图校验。
/// 预期结果：中英文大小写和无空格的明确指令通过；另一个Agent、赞同和假设句拒绝。
#[test]
fn explicit_named_agent_instruction_is_authorized() {
    let command = AdapterCommand::CreateAgent {
        profile: norma::protocol::AgentProfile::Codex,
        task: Some("实现登录校验，保留约束并交付测试证据".into()),
    };
    assert!(
        realtime_command_authorized(&command, "请让 Codex 实现登录校验，保留约束并交付测试证据"),
        "真实口头派工包含目标Agent，不能误留为讨论"
    );
    assert!(
        realtime_command_authorized(&command, "让codex修复登录测试"),
        "中文转写没有空格时仍应识别目标"
    );
    assert!(
        !realtime_command_authorized(&command, "让 Claude 实现登录校验"),
        "明确指定另一Agent不能授权当前目标"
    );
    assert!(
        !realtime_command_authorized(&command, "让 Codex 实现方案不错"),
        "名词评价仍不是执行意图"
    );
    assert!(
        !realtime_command_authorized(&command, "如果让 Codex 实现登录校验"),
        "假设句不能通过委托前缀放行"
    );
    assert!(
        !realtime_command_authorized(&command, "让 Codexx 实现登录校验"),
        "相似目标名不能匹配Codex"
    );
}

/// 预览动作可以直接以动词开头，不能被创建类别收紧误伤。
/// 数据构造：指向同一隔离README的CreatePreview，中英文预览原文和仅提及用途的原文。
/// 执行过程：调用共享授权门槛，分别验证两个明确请求与一个非请求。
/// 预期结果：明确预览可用，终端用途中的“预览”仍不能创建预览卡片。
#[test]
fn explicit_preview_verbs_keep_working() {
    let command = AdapterCommand::CreatePreview {
        path: "/isolated/README.md".into(),
        explicit_type: None,
    };
    for input in ["预览 /isolated/README.md", "preview /isolated/README.md"] {
        assert!(
            realtime_command_authorized(&command, input),
            "明确预览应允许：{input}"
        );
    }
    assert!(
        !realtime_command_authorized(&command, "创建一个终端用于预览README"),
        "用途不是创建预览的请求"
    );
    assert!(
        !realtime_command_authorized(&AdapterCommand::CreateTerminal, "preview terminal"),
        "预览动作不能创建终端"
    );
}

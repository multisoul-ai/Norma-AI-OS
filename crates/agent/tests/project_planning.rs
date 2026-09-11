use agent::ProjectPlanDraft;
use norma::mobile::CardSnapshot;

/// 真实配置模型在两次方向输入后返回合法结构并绑定指定 Agent；内容质量须另行阅读验收。
///
/// 数据构造（含关键数值的推导过程）：
///   可用卡片 = 2（7/Codex 负责连接，8/Claude 负责界面）；均为空闲受控卡片。
///   真实调用 = 2（先网络，后视觉）；每次明确只要 1 条 Todo。
///   超时沿用正式生成器 20s/次，2 次共最多 40s，不扩大手机回执预算。
/// 执行过程：同一模型/上下文先生成网络建议，再改变方向生成视觉建议；保存无凭据的实际结果供阅读。
/// 预期结果：网络只绑定卡片7、视觉只绑定卡片8；不能互换、重复相同指令或虚构运行实例。
/// 限制：不同指令不代表语义正确；不通过词匹配把方向遵循、事实准确或验收标准判为合格。
#[tokio::test]
#[ignore = "calls NORMA_LIVE_BASE_URL/MODEL (PROTOCOL=openai or ollama); cloud requires NORMA_LIVE_API_KEY; optional NORMA_LIVE_PLAN_EVIDENCE records synthetic results"]
async fn live_planner_changes_agent_with_user_direction() {
    use agent::{generate_project_plan, ProviderConfig, ProviderProtocol};
    let model = std::env::var("NORMA_LIVE_MODEL").expect("明确模型");
    let (protocol, key) = match std::env::var("NORMA_LIVE_PROTOCOL").as_deref() {
        Ok("ollama") => (ProviderProtocol::Ollama, None),
        Ok("openai") | Err(_) => (
            ProviderProtocol::OpenAiCompatible,
            Some(std::env::var("NORMA_LIVE_API_KEY").expect("云凭据仅由进程环境传入")),
        ),
        Ok(_) => panic!("仅支持明确的 openai 或 ollama 协议"),
    };
    let config = ProviderConfig::new(
        protocol,
        std::env::var("NORMA_LIVE_BASE_URL").expect("明确服务地址"),
        model.clone(),
        key,
    );
    let cards = [
        CardSnapshot {
            node_id: 7,
            profile: "Codex".into(),
            title: "连接回归".into(),
            mobile_control: true,
            can_interrupt: None,
            task_timing: None,
            task_progress: None,
            task_summary: None,
            state: "idle".into(),
            run_id: None,
            session_id: None,
            turn_id: None,
            task_title: None,
        },
        CardSnapshot {
            node_id: 8,
            profile: "Claude".into(),
            title: "视觉优化".into(),
            mobile_control: true,
            can_interrupt: None,
            task_timing: None,
            task_progress: None,
            task_summary: None,
            state: "idle".into(),
            run_id: None,
            session_id: None,
            turn_id: None,
            task_title: None,
        },
    ];
    let context = "这是验收样例：SwiftUI页面原型已实现，断线重连和深色布局均尚未验收。两张卡片均空闲，尚未执行任务。";
    let network_direction = "这轮只生成一条网络验收建议，交给连接回归卡片7：覆盖断线重连，不安排视觉工作，明确验收标准。";
    let visual_direction =
        "调整方向：这轮先放下网络，只生成一条深色布局检查建议，交给视觉优化卡片8，明确验收标准。";
    let network = generate_project_plan(&config, network_direction, context, &cards, None).await;
    let visual = generate_project_plan(&config, visual_direction, context, &cards, None).await;
    if let Some(path) = std::env::var_os("NORMA_LIVE_PLAN_EVIDENCE") {
        let evidence = serde_json::json!({"model":model,"context":context,
            "network_direction":network_direction,"network":network,
            "visual_direction":visual_direction,"visual":visual});
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&evidence).expect("序列化样例结果"),
        )
        .expect("保存不含凭据的实际规划结果");
    }
    let network = network.expect("网络规划应在正式20s预算内返回合法建议");
    let visual = visual.expect("方向调整应在正式20s预算内返回合法建议");
    assert_eq!(network.todos.len(), 1, "用户本轮只要求一条网络建议");
    assert_eq!(
        network.todos[0].target, cards[0],
        "网络建议必须绑定卡片7原始身份"
    );
    assert_ne!(
        network.todos[0].title, "简洁行动",
        "不能把提示词示例当成实际任务标题"
    );
    assert_ne!(
        network.todos[0].target.node_id, 8,
        "网络工作不能改派视觉卡片"
    );
    assert_eq!(visual.todos.len(), 1, "调整后也只要求一条视觉建议");
    assert_eq!(
        visual.todos[0].target, cards[1],
        "视觉建议必须绑定卡片8原始身份"
    );
    assert_ne!(
        visual.todos[0].title, "简洁行动",
        "方向调整后也必须生成实际任务标题"
    );
    assert_ne!(
        visual.todos[0].instruction, network.todos[0].instruction,
        "改变方向不能重复旧的执行指令"
    );
}

/// 规划只向已配置Provider请求一次JSON建议，不能给模型注册执行工具。
///
/// 数据构造：回环OpenAI兼容fixture返回一个合法计划；输入包含用户方向和当前工作摘要。
/// 执行过程：正式ModelFactory/Rig请求 → 检查HTTP正文 → 返回建议 → 精确绑定卡片。
/// 预期结果：请求包含方向/摘要但tools为空；返回真实fixture内容，无自动派发或伪完成。
#[tokio::test]
async fn planning_uses_configured_model_without_execution_tools() {
    check_planning_transport(false, false).await;
}

/// Ollama 规划必须请求原生 JSON schema，不能仅依靠模型遵守文字格式提示。
/// 数据构造：1 次回环 Ollama 响应，1 条合法 Todo；schema 至多 8 条，沿用手机计划上限。
/// 执行过程：正式 ModelFactory/Rig 发送 /api/chat，检查 format 与 tools，再解析回复。
/// 预期结果：format 限制对象字段/必填项，tools 为空；目标和指令仍与服务响应准确一致。
#[tokio::test]
async fn ollama_planning_requests_native_schema_without_tools() {
    check_planning_transport(true, false).await;
}

/// 结构化优先事项进入正式模型请求，并在实际响应中稳定前移。
/// 数据构造：同一Codex卡7有2项建议；选择第2项“验证登录”，模型先返回“整理界面”。
/// 执行过程：正式Provider请求 → 核对priority_todo原身份/标题/指令 → 解析两条响应 → 优先项前移。
/// 预期结果：方向保持原文、无执行工具；所选项第1，其余项保留，执行指令来自新回复。
#[tokio::test]
async fn planning_transports_priority_and_reorders_model_response() {
    check_planning_transport(false, true).await;
}

async fn check_planning_transport(native: bool, prioritized: bool) {
    use agent::{generate_project_plan, ProviderConfig, ProviderProtocol};
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("回环Provider");
    let address = listener.local_addr().expect("端口");
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("单次请求");
        //5s exceeds one local HTTP exchange; fixture cannot block on an incomplete body indefinitely.
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("有界读");
        let mut reader = std::io::BufReader::new(&mut socket);
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("HTTP头");
            assert!(!line.is_empty(), "不能在完整请求前EOF");
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().expect("长度");
            }
        }
        assert!(
            length > 0 && length < 128 * 1024,
            "规划请求必须有正文且有界"
        );
        let mut body = vec![0; length];
        reader.read_exact(&mut body).expect("完整正文");
        let request: serde_json::Value = serde_json::from_slice(&body).expect("JSON请求");
        assert_eq!(request["model"], "plan-fixture", "必须使用Mac配置的模型");
        assert!(
            request["tools"].is_null() || request["tools"].as_array().is_some_and(Vec::is_empty),
            "生成建议不得注册派发或文件工具"
        );
        if native {
            assert_eq!(
                request["format"]["type"], "object",
                "Ollama 必须使用原生对象 schema"
            );
            assert_eq!(
                request["format"]["additionalProperties"], false,
                "不能让模型添加执行状态等字段"
            );
            assert_eq!(
                request["format"]["required"],
                serde_json::json!(["summary", "todos"]),
                "摘要和 Todo 必須同时存在"
            );
            assert_eq!(
                request["format"]["properties"]["todos"]["maxItems"], 8,
                "原生约束与8项计划上限一致"
            );
        } else {
            assert!(
                request["format"].is_null(),
                "Ollama 参数不能发送给 OpenAI 兼容服务"
            );
        }
        let messages = request["messages"].to_string();
        assert!(messages.contains("先验证登录"), "用户方向必须进入模型输入");
        assert!(
            messages.contains("当前登录实现已提交"),
            "不能脱离项目进度生成静态Todo"
        );
        let user = request["messages"]
            .as_array()
            .expect("消息数组")
            .iter()
            .find(|message| message["role"] == "user")
            .expect("用户输入");
        let input: serde_json::Value =
            serde_json::from_str(user["content"].as_str().expect("JSON用户正文"))
                .expect("结构化输入");
        assert_eq!(input["direction"], "先验证登录", "偏好不得拼入方向原文");
        if prioritized {
            assert_eq!(
                input["priority_todo"]["target"]["node_id"], 7,
                "优先项绑定原卡7"
            );
            assert_eq!(
                input["priority_todo"]["title"], "验证登录",
                "模型必须收到所选事项标题"
            );
            assert_eq!(
                input["priority_todo"]["instruction"], "原计划的登录验证",
                "优先项的原指令不可遗漏"
            );
        } else {
            assert!(
                input["priority_todo"].is_null(),
                "未指定优先事项不得伪造偏好"
            );
        }
        let content = if prioritized {
            r#"{"summary":"先确认登录可靠性","todos":[{"node_id":7,"title":"整理界面","instruction":"核对布局"},{"node_id":7,"title":"验证登录","instruction":"检查登录错误分支。"}]}"#
        } else {
            r#"{"summary":"先确认登录可靠性","todos":[{"node_id":7,"title":"验证登录","instruction":"检查登录错误分支。"}]}"#
        };
        let reply = if native {
            serde_json::json!({"model":"plan-fixture","created_at":"2026-09-08T00:00:00Z","message":{"role":"assistant","content":content},"done":true})
        } else {
            serde_json::json!({"id":"fixture","object":"chat.completion","created":1,"model":"plan-fixture","choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":"stop"}]})
        }.to_string();
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",reply.len(),reply).expect("响应");
    });
    let config = ProviderConfig::new(
        if native {
            ProviderProtocol::Ollama
        } else {
            ProviderProtocol::OpenAiCompatible
        },
        if native {
            format!("http://{address}")
        } else {
            format!("http://{address}/v1")
        },
        "plan-fixture",
        None,
    );
    let card = CardSnapshot {
        node_id: 7,
        profile: "Codex".into(),
        title: "开发".into(),
        mobile_control: true,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: None,
        state: "idle".into(),
        run_id: None,
        session_id: None,
        turn_id: None,
        task_title: None,
    };
    let priority = agent::PlanTodo {
        target: card.clone(),
        title: "验证登录".into(),
        instruction: "原计划的登录验证".into(),
    };
    let plan = generate_project_plan(
        &config,
        "先验证登录",
        "当前登录实现已提交",
        std::slice::from_ref(&card),
        prioritized.then_some(&priority),
    )
    .await
    .expect("正式模型适配");
    server.join().expect("Provider回收");
    assert_eq!(plan.summary, "先确认登录可靠性", "必须保留模型的实际建议");
    if prioritized {
        assert_eq!(plan.todos.len(), 2, "排序不删除其他建议");
        assert_eq!(plan.todos[0].title, "验证登录", "优先项必须排首位");
        assert_eq!(plan.todos[1].title, "整理界面", "原首项顺延保留");
        assert_ne!(
            plan.todos[0].instruction, "原计划的登录验证",
            "必须保留模型结合新方向生成的指令"
        );
    }
    assert_eq!(plan.todos[0].target, card, "只能安排已提供的准确卡片");
    assert_eq!(
        plan.todos[0].instruction, "检查登录错误分支。",
        "不能替换为固定任务"
    );
}

/// Norma的计划只建议已提供的受控卡片，不能伪造执行者或执行结果。
///
/// 数据构造：卡片7为Codex/run-a，卡片8为未受控Claude；回复只含一条建议，不含执行状态。
/// 执行过程：解析合法JSON → 绑定卡片7 → 验证原生CLI首次输入前无session → 尝试错身份/卡片8/9/完成字段。
/// 预期结果：合法建议及原生待输入卡保留准确运行；孤立session、无授权目标、额外状态均拒绝。
#[test]
fn plans_bind_only_supplied_controlled_cards_without_claiming_execution() {
    let codex = CardSnapshot {
        node_id: 7,
        profile: "Codex".into(),
        title: "开发".into(),
        mobile_control: true,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: None,
        state: "ready".into(),
        run_id: Some("run-a".into()),
        session_id: Some("session-a".into()),
        turn_id: None,
        task_title: Some("实现登录".into()),
    };
    let mut claude = codex.clone();
    claude.node_id = 8;
    claude.profile = "Claude".into();
    claude.mobile_control = false;
    let cards = [codex.clone(), claude];
    let json = r#"{"summary":"下一步验证登录","todos":[{"node_id":7,"title":"补充验证","instruction":"为登录流程补充测试。"}]}"#;
    let draft = ProjectPlanDraft::from_response(json, &cards).expect("合法建议");
    assert_eq!(draft.summary, "下一步验证登录", "不能生成静态占位总结");
    assert_eq!(draft.todos.len(), 1, "不能把建议展开为额外任务");
    assert_eq!(
        draft.todos[0].target, codex,
        "目标必须由Mac提供的准确快照绑定"
    );
    assert_eq!(
        draft.todos[0].instruction, "为登录流程补充测试。",
        "指令不能被摘要替代"
    );
    let mut native = codex.clone();
    native.session_id = None;
    let first = ProjectPlanDraft::from_response(json, &[native]).expect("原生待输入卡应能接收计划");
    assert_eq!(
        first.todos[0].target.run_id.as_deref(),
        Some("run-a"),
        "必须绑定已有PTY运行"
    );
    assert!(
        first.todos[0].target.session_id.is_none(),
        "首次提交前不能伪造会话"
    );
    let mut orphan = codex.clone();
    orphan.run_id = None;
    assert!(
        ProjectPlanDraft::from_response(json, &[orphan]).is_err(),
        "会话不可脱离运行身份"
    );
    assert!(
        ProjectPlanDraft::from_response(&json.replace(":7,", ":8,"), &cards).is_err(),
        "未受控Claude不能成为建议执行者"
    );
    assert!(
        ProjectPlanDraft::from_response(&json.replace(":7,", ":9,"), &cards).is_err(),
        "不存在的卡片不能由模型创建"
    );
    assert!(
        ProjectPlanDraft::from_response(
            &json.replace("\"title\":", "\"state\":\"completed\",\"title\":"),
            &cards
        )
        .is_err(),
        "模型不能自行声称已完成"
    );
}

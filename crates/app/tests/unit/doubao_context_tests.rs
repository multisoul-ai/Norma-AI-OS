use super::{plan::chat, ContextRequest, ContextRequests};
use crate::app::MultisoulApp;
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect};
use gpui::{TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use ui::i18n::LocalizedText;

/// DB-CONTEXT-DESKTOP-1：豆包模型请求也应看到当前桌面工作空间和可见项目。
///
/// 数据构造：画布项目名设为旧值，桌面快照指向工作空间 dw-42 的 area-7。
/// 执行过程：通过生产上下文请求通道构造一次模型请求。
/// 预期结果：system 含当前稳定身份，不再把旧项目写成当前项目。
#[tokio::test]
async fn doubao_model_request_uses_current_desktop_location() {
    let (context, requests): (ContextRequests, _) = async_channel::bounded(1);
    let responder = tokio::spawn(async move {
        let ContextRequest::Model(response) = requests.recv().await.expect("模型请求应到达");
        let _ = response.send((
            agent::CurrentCanvasContext {
                project_name: "上次项目".into(),
                workspace_root: "/old-project".into(),
                total_cards: 0,
                cards: Vec::new(),
                conversation_tails: Vec::new(),
            },
            Vec::new(),
            Some(agent::DesktopContext::ProjectCanvas {
                workspace_id: "dw-42".into(),
                workspace_name: "产品研发".into(),
                area_id: "area-7".into(),
                project_name: "Norma OS".into(),
            }),
        ));
    });
    let mut history = vec![json!({"role":"user","content":"看看当前项目"})];

    let body = chat::request_with_current_context(&context, "", &mut history, true, true)
        .await
        .expect("本轮请求应构造成功");
    responder.await.expect("上下文提供方应完成");
    let system = body["messages"][0]["content"]
        .as_str()
        .expect("system 消息应存在");
    assert!(
        system.contains("当前桌面工作空间：产品研发（id=dw-42）"),
        "豆包应获得真实桌面工作空间：{system}"
    );
    assert!(
        system.contains("当前项目：Norma OS（area_id=area-7）"),
        "豆包应获得当前可见项目：{system}"
    );
    assert!(
        !system.contains("当前项目：上次项目"),
        "旧画布项目不得冒充当前项目：{system}"
    );
    let tools = body["tools"].as_array().expect("桌面语音应提供只读工具");
    assert!(
        tools
            .iter()
            .any(|tool| tool["function"]["name"] == "workspace_snapshot"),
        "无项目 root 的桌面语音仍须暴露工作空间快照工具"
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool["function"]["name"] == "create_browser"),
        "桌面豆包应能在右侧打开网页标签"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool["function"]["name"] == "assign_agent_task"),
        "尚未接通跨项目派活时，不应暴露无法正确路由的旧项目派活工具"
    );
}

fn request(context: &ContextRequests, cx: &VisualTestContext) -> Result<Value, LocalizedText> {
    // 单个请求、单个回执，容量 1 足够；调度器推进到无待执行任务，无真实网络或 sleep。
    let (result_tx, result_rx) = async_channel::bounded(1);
    let context = context.clone();
    cx.executor()
        .spawn(async move {
            let mut history = vec![json!({"role":"user","content":"读取最新卡片"})];
            let result = chat::request_with_current_context(
                &context,
                "\n本轮人设与记忆目录",
                &mut history,
                true,
                false,
            )
            .await;
            result_tx.send(result).await.expect("请求结果应回到测试");
        })
        .detach();
    cx.run_until_parked();
    result_rx
        .try_recv()
        .expect("上下文请求应完成而不是一直等待")
}

/// 通话期间新增 Agent 后，下一次模型请求必须读取最新画布。
///
/// 数据构造（含关键数值的推导过程）：
///   初始卡片 = 0；新增 Chloe/Codex = 1；最终卡片 = 0 + 1 = 1。
///   卡片区域 = 640 × 360，仅提供合法几何，不参与提示词判断。
///   模型请求 = 2（新增前 + 新增后），复用同一通话上下文通道。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建隔离 App 与空画布，启动生产 GPUI 上下文通道。
///   2. 通过生产请求构造入口读取首轮提示词，确认暂无卡片。
///   3. 向 Runtime 加入 Chloe 卡片，保持相同通话与历史读取入口。
///   4. 构造下一轮模型请求，检查最新卡片与本轮附加说明。
///
/// 预期结果：
///   - 正断言：新增后提示词包含 Chloe 的 AgentCard 和本轮附加说明。
///   - 负断言：新增前不存在 Chloe，新增后不再包含“暂无卡片”。
#[test]
fn voice_context_refreshes_cards_between_requests() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("隔离工作区应可创建");
    let (app, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let context = cx.update(|_, app_cx| {
        app.update(app_cx, |app, cx| {
            app.start_doubao_context_requests(
                app.doubao.generation,
                Some(workspace.path().display().to_string()),
                cx,
            )
        })
    });

    let first = request(&context, cx).expect("首轮空画布请求应成功");
    let first_system = first["messages"][0]["content"]
        .as_str()
        .expect("首轮应包含系统提示词");
    assert!(first_system.contains("暂无卡片"), "首轮应如实报告空画布");
    assert!(
        !first_system.contains("Chloe"),
        "尚未创建的 Chloe 不应提前出现"
    );

    app.update(cx, |app, _| {
        app.runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: Some("Chloe".into()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 360.0),
        );
    });
    let second = request(&context, cx).expect("新增卡片后的请求应成功");
    let second_system = second["messages"][0]["content"]
        .as_str()
        .expect("下一轮应包含系统提示词");
    assert!(
        second_system.contains("Chloe 的 AgentCard"),
        "同一通话下一轮必须看到新卡片"
    );
    assert!(
        !second_system.contains("暂无卡片"),
        "新卡片存在时不得沿用空画布快照"
    );
    assert!(
        second_system.contains("本轮人设与记忆目录"),
        "更新快照时不能丢弃本轮附加说明"
    );
}

/// 同一卡片完成新的回复后，下一次模型请求必须替换旧回复。
///
/// 数据构造（含关键数值的推导过程）：
///   卡片 = 1（Chloe/Codex）；会话 = 1（same-session），两次更新复用相同键。
///   时序 = 1 → 2，仅表示先后顺序，无实际时钟依赖。
///   模型请求 = 2（旧回复 + 更新后），期望最后正文只有 LATEST_REPLY。
///   卡片区域 = 640 × 360，提供合法几何，不影响会话读取。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建隔离 App，向当前画布添加 Chloe，并载入旧会话尾部。
///   2. 启动生产上下文通道，读取首轮模型请求中的旧回复。
///   3. 用同一卡片与会话键更新最后完整回复，再构造下一次模型请求。
///
/// 预期结果：
///   - 正断言：首轮包含 OLDER_REPLY，下一轮包含 LATEST_REPLY。
///   - 负断言：首轮不能提前包含新回复，下一轮不能混入旧回复。
#[test]
fn voice_context_refreshes_completed_reply_between_requests() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("隔离工作区应可创建");
    let (app, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let node_id = app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx);
        app.runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: Some("Chloe".into()),
                definition_id: Some(1),
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 360.0),
        )
    });
    let mut tail = agent::AgentConversationTail {
        definition_id: 1,
        node_id,
        session_id: "same-session".into(),
        agent_name: "Chloe".into(),
        profile: norma::protocol::AgentProfile::Codex,
        task_state: agent::AgentConversationTaskState::Done,
        last_user_message: Some("检查项目".into()),
        last_agent_message: Some("OLDER_REPLY".into()),
        updated_at_unix: 1,
        availability: agent::AgentConversationAvailability::Available,
        user_message_truncated: false,
        agent_message_truncated: false,
    };
    app.update(cx, |app, _| {
        app.conversation_tail_memory.load_stored(vec![tail.clone()]);
    });
    let context = cx.update(|_, app_cx| {
        app.update(app_cx, |app, cx| {
            app.start_doubao_context_requests(
                app.doubao.generation,
                Some(workspace.path().display().to_string()),
                cx,
            )
        })
    });
    let first = request(&context, cx).expect("旧回复请求应成功");
    let first_system = first["messages"][0]["content"]
        .as_str()
        .expect("首轮应包含系统提示词");
    assert!(
        first_system.contains("OLDER_REPLY"),
        "首轮应包含已有完整回复"
    );
    assert!(
        !first_system.contains("LATEST_REPLY"),
        "尚未完成的新回复不能提前出现"
    );

    tail.last_agent_message = Some("LATEST_REPLY".into());
    tail.updated_at_unix = 2;
    app.update(cx, |app, _| {
        app.conversation_tail_memory.load_stored(vec![tail]);
    });
    let second = request(&context, cx).expect("最新回复请求应成功");
    let second_system = second["messages"][0]["content"]
        .as_str()
        .expect("下一轮应包含系统提示词");
    assert!(
        second_system.contains("LATEST_REPLY"),
        "同一通话下一轮必须读取最新回复"
    );
    assert!(
        !second_system.contains("OLDER_REPLY"),
        "更新后的系统上下文不得沿用旧回复"
    );
}

/// 结束通话后，迟到的上下文请求必须失败，不能再生成可发送的模型请求。
///
/// 数据构造（含关键数值的推导过程）：
///   工作区 = 1；通话上下文通道 = 1；请求 = 2（正常 + 结束后）。
///   初始 generation = 0；stop 后 generation = 0 + 1 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建隔离 App 和生产上下文通道，确认正常请求成功。
///   2. 调用生产 stop，使旧通话 generation 失效。
///   3. 再通过旧通道请求上下文，检查明确失败且不会一直等待。
///
/// 预期结果：
///   - 正断言：正常请求成功，迟到请求返回通话已结束。
///   - 负断言：结束后不得返回可以继续发给模型的请求正文。
#[test]
fn voice_context_rejects_requests_after_call_ends() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("隔离工作区应可创建");
    let (app, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let context = cx.update(|_, app_cx| {
        app.update(app_cx, |app, cx| {
            app.start_doubao_context_requests(
                app.doubao.generation,
                Some(workspace.path().display().to_string()),
                cx,
            )
        })
    });
    assert!(request(&context, cx).is_ok(), "通话结束前上下文必须可读取");
    app.update(cx, |app, _| app.doubao.stop());
    let result = request(&context, cx);
    assert!(result.is_err(), "旧通话不得继续返回有效模型请求");
    assert_eq!(
        result.expect_err("旧通话请求必须失败"),
        LocalizedText::new("doubao.call_ended"),
        "迟到上下文请求必须明确指向通话已结束"
    );
}

/// 项目发生变化后，即使通话 generation 未变，也不能把新项目数据发给旧通话。
///
/// 数据构造（含关键数值的推导过程）：
///   工作区 = 2（原项目 + 新项目）；上下文通道 = 1（绑定原项目）。
///   请求 = 2（切换前成功 + 切换后拒绝）；generation 保持不变。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在原项目创建 App 与生产上下文通道，确认首轮请求成功。
///   2. 仅替换 Runtime 工作区路径，排除 generation 变化的影响。
///   3. 通过旧通道发起下一轮请求，检查项目身份校验的结果。
///
/// 预期结果：
///   - 正断言：首轮成功，切换后返回通话已结束。
///   - 负断言：旧通话不能得到新项目的提示词，也不能一直等待。
#[test]
fn voice_context_rejects_requests_after_project_changes() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("原项目应可创建");
    let next_workspace = tempfile::TempDir::new().expect("新项目应可创建");
    let (app, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let context = cx.update(|_, app_cx| {
        app.update(app_cx, |app, cx| {
            app.start_doubao_context_requests(
                app.doubao.generation,
                Some(workspace.path().display().to_string()),
                cx,
            )
        })
    });
    assert!(request(&context, cx).is_ok(), "切换前应读取原项目上下文");
    app.update(cx, |app, _| {
        app.runtime
            .set_workspace_root(next_workspace.path().to_path_buf());
    });
    let result = request(&context, cx);
    assert!(result.is_err(), "项目已变化时旧通话不得获得新项目数据");
    assert_eq!(
        result.expect_err("旧项目通道必须拒绝上下文读取"),
        LocalizedText::new("doubao.call_ended"),
        "项目切换后应明确终止旧通话请求"
    );
}

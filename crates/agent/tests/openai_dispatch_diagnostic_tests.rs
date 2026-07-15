pub mod support;

use agent::{parse_openai_stream, AgentCommand, AgentEvent, ProviderStreamEvent, ProviderToolCall};
use norma::protocol::CardCommand;
use support::{
    init_trace, response, system, task_id_from, text_response, tool, RecordingHost,
    ScriptedProvider,
};

/// D-1：OpenAI 流式 `card_task` 必须穿过统一 AgentSystem 并启动 Card Subagent。
///
/// 数据构造：Dialogue SSE 把一个 `card_task start` 的参数拆成两个 chunk；Card Subagent
/// 随后返回一个 `create_card`，再返回终态文本。
/// 执行过程：先用生产 OpenAI stream parser 归一化响应，再提交 UserTurn 并持续 pump 到空闲。
/// 预期结果：任务经历 queued/running/succeeded，且 CardToolHost 恰好执行一次创建终端。
#[test]
fn openai_stream_card_task_reaches_card_subagent_and_tool_host() {
    init_trace();
    let raw = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"dialogue-call-1\",\"function\":{\"name\":\"card_task\",\"arguments\":\"{\\\"action\\\":\\\"start\\\",\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"intent\\\":\\\"创建终端\\\"}\"}}]}}]}\n\n",
        "data: [DONE]\n\n"
    );
    let mut stream_events = Vec::new();
    let dialogue_response = parse_openai_stream(raw, &mut |event| stream_events.push(event))
        .expect("合规 OpenAI SSE 应解析为统一 ProviderResponse");
    let card = ScriptedProvider::new([
        response([tool("card-call-1", "create_card", r#"{"kind":"terminal"}"#)]),
        text_response("已创建终端"),
    ]);
    let mut agent_system = system(
        ScriptedProvider::new([dialogue_response]),
        card,
        RecordingHost::default(),
    );

    let mut events = agent_system
        .submit(AgentCommand::UserTurn("创建终端".to_string()))
        .expect("OpenAI tool call 应成功提交给 AgentSystem");
    assert_eq!(
        task_id_from(&events),
        1,
        "Dialogue tool call 必须创建 task_id=1 的 queued 任务"
    );
    while agent_system.pending_task_count() > 0 {
        events.extend(agent_system.pump());
    }

    assert!(
        stream_events.iter().any(|event| matches!(
            event,
            ProviderStreamEvent::ToolCall(ProviderToolCall { id, name, .. })
                if id == "dialogue-call-1" && name == "card_task"
        )),
        "OpenAI stream parser 必须产出完整 card_task 调用"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskRunning { task_id: 1 })),
        "后续 pump 必须启动独立 Card Subagent"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskSucceeded { task_id: 1, .. })),
        "Card Subagent 完成后必须产生 succeeded 终态"
    );
    assert_eq!(
        agent_system.card_tool_host().commands.len(),
        1,
        "合规 OpenAI tool-call 链路应且仅应执行一个底层卡片工具"
    );
    assert!(
        matches!(
            agent_system.card_tool_host().commands[0].command,
            CardCommand::Create { .. }
        ),
        "Card Subagent 的 create_card 必须抵达 CardToolHost"
    );
}

mod support;

use agent::{
    AgentCommand, AgentProvider, AgentSystem, CardTaskBudget, MemoryJournal, ProviderError,
    ProviderRequest, ProviderResponse, ProviderStreamEvent,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use support::{RecordingHost, ScriptedProvider};

struct CancellationObservingProvider;

impl AgentProvider for CancellationObservingProvider {
    fn complete(&mut self, _request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        Err(ProviderError::Message(
            "test must use the cancellation-aware stream path".to_string(),
        ))
    }

    fn stream_with_cancellation(
        &mut self,
        _request: &ProviderRequest,
        cancelled: Option<&AtomicBool>,
        emit: &mut dyn FnMut(ProviderStreamEvent),
    ) -> Result<ProviderResponse, ProviderError> {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err(ProviderError::Message(
                "dialogue stream cancelled".to_string(),
            ));
        }
        emit(ProviderStreamEvent::TextDelta(
            "这段文本不该到达用户。".to_string(),
        ));
        Ok(ProviderResponse {
            text: "这段文本不该到达用户。".to_string(),
            tool_calls: Vec::new(),
        })
    }
}

/// CAN-1：用户打断先置位取消令牌时，Provider 不得再输出旧对话 delta。
///
/// 数据构造：取消 AtomicBool 初始为 true；Provider 在令牌为 true 时返回稳定 cancellation
/// 错误，否则会刻意 emit 一条不该出现的旧文本。
/// 执行过程：将该令牌注入 AgentSystem，再提交普通 ModelOnly UserTurn。
/// 预期结果：submit 返回取消错误；不得生成任何 DialogueTextDelta 或 CardTask。
#[test]
fn dialogue_stream_cancellation_is_observed_before_any_delta() {
    let cancelled = Arc::new(AtomicBool::new(true));
    let mut system = AgentSystem::new(
        "project-a",
        CancellationObservingProvider,
        ScriptedProvider::default(),
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    )
    .with_live_dialogue_cancellation(cancelled);

    let result = system.submit(AgentCommand::UserTurn("解释无限画布".to_string()));

    assert!(
        result.is_err(),
        "已取消的对话流必须停止，而不是继续返回旧文本"
    );
    assert_eq!(
        system.pending_task_count(),
        0,
        "被打断的说明性对话不得创建 CardTask"
    );
    assert!(
        system.events().is_empty(),
        "取消发生在首个 delta 前时不应留下可播旧回复"
    );
}

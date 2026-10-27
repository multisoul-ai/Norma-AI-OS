use super::*;
use agent::{AdapterCommand, AdapterResult, AgentCardAdapter, ToolContext};
use futures::future::BoxFuture;
use std::sync::Arc;

pub(crate) fn wrap(
    delegate: Arc<dyn AgentCardAdapter>,
    service: SharedContext,
) -> Arc<dyn AgentCardAdapter> {
    Arc::new(ContextAdapter { delegate, service })
}
struct ContextAdapter {
    delegate: Arc<dyn AgentCardAdapter>,
    service: SharedContext,
}

impl ContextAdapter {
    async fn checkpoint(&self, context: &ToolContext) -> Result<(), AdapterError> {
        if context.cancellation.is_cancelled() {
            return Err(AdapterError::Cancelled);
        }
        let result = self
            .delegate
            .execute(
                AdapterCommand::ContextCheckpoint {
                    scope: self.service.scope().clone(),
                },
                context,
            )
            .await?;
        if result.status != "succeeded" {
            return Err(AdapterError::Rejected(
                "context_checkpoint_unconfirmed".into(),
            ));
        }
        Ok(())
    }
    async fn frame(&self, context: &ToolContext) -> Result<ContextFrame, AdapterError> {
        self.checkpoint(context).await?;
        let source = context;
        let context = context.clone();
        let origin = match context.input_source {
            agent::AgentInputSource::Voice => ContextOrigin::Voice,
            agent::AgentInputSource::DoubaoVoice => ContextOrigin::DoubaoVoice,
            agent::AgentInputSource::GptLiveVoice => ContextOrigin::GptLiveVoice,
            agent::AgentInputSource::Other => ContextOrigin::Dock,
            _ => return Err(AdapterError::Rejected("群聊上下文保持独立作用域".into())),
        };
        let frame = self
            .service
            .run(None, move |access| {
                let input = access.record_input(&context.turn_id, origin, &context.user_input)?;
                Ok(ContextFrame {
                    input: input.reference,
                    snapshot: access.snapshot()?,
                })
            })
            .await?;
        self.checkpoint(source).await?;
        Ok(frame)
    }
}

impl AgentCardAdapter for ContextAdapter {
    fn memory_context(
        &self,
    ) -> BoxFuture<'_, Result<Option<agent::MemorySessionContext>, AdapterError>> {
        self.delegate.memory_context()
    }
    fn shared_context<'a>(
        &'a self,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<ContextFrame>, AdapterError>> {
        Box::pin(async move { self.frame(context).await.map(Some) })
    }
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            match command {
                AdapterCommand::ReadSharedContext { reference, offset } => {
                    self.checkpoint(context).await?;
                    let record = self
                        .service
                        .run(None, move |access| access.read(&reference))
                        .await?;
                    self.checkpoint(context).await?;
                    context_read_result(&record, offset)
                }
                AdapterCommand::UpdateWorkingState(update) => {
                    let frame = self.frame(context).await?;
                    if context.tool_call_id.is_empty() {
                        return Err(AdapterError::Rejected("状态变更缺少调用身份".into()));
                    }
                    use sha2::{Digest, Sha256};
                    // Retry identity belongs to the authenticated candidate, not a provider call.
                    let operation = format!(
                        "candidate-{:x}",
                        Sha256::digest(serde_json::to_vec(&update).map_err(failed)?)
                    );
                    let record = self
                        .service
                        .run(Some(operation.clone()), move |access| {
                            access.update(&operation, &frame.input, &update)
                        })
                        .await?;
                    Ok(AdapterResult::succeeded("工作状态已保存").with_snapshot(
                        serde_json::json!({"saved":true,"reference":record.reference}),
                    ))
                }
                AdapterCommand::HandoffToBrain { request } => {
                    let frame = self.frame(context).await?;
                    self.delegate
                        .execute(
                            AdapterCommand::HandoffWithContext {
                                request,
                                reference: frame.input,
                            },
                            context,
                        )
                        .await
                }
                other => self.delegate.execute(other, context).await,
            }
        })
    }
}

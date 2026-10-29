use agent::NormaToolRuntime;
use async_channel::Sender;
use reqwest::Client;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

use super::{direct, plan_url, tts};
use crate::app::doubao::{
    playback::PlaybackQueue, tools, ContextRequest, ContextRequests, DoubaoStatus,
};

#[cfg(test)]
#[path = "../../../../tests/unit/doubao_memory_revision_tests.rs"]
mod memory_revision_tests;
#[cfg(test)]
#[path = "../../../../tests/unit/doubao_reply_publish_tests.rs"]
mod publish_tests;
#[cfg(test)]
#[path = "../../../../tests/unit/doubao_shared_context_tests.rs"]
mod shared_tests;

const CHAT_MODEL: &str = "doubao-seed-2.0-mini";
// Seed 2.0 Mini: 256,000-token context, 224,000-token maximum input.
// https://docs.volcengine.com/docs/82379/1330310
// Keep the conservative 2 serialized bytes/token estimate and reserve one
// bounded memory result (8,192 B). The remaining 32,000 tokens cover our
// at-most-1,024-token reply without exceeding the model's total context.
pub(crate) const REQUEST_INPUT_BYTES: usize = 224_000 * 2 - agent::MEMORY_TOOL_BYTES;

pub(super) struct ChatRuntime {
    pub(super) client: Client,
    pub(super) key: String,
    pub(super) voice: &'static ::tts::providers::doubao::DoubaoVoice,
    pub(super) context: ContextRequests,
    pub(super) history: Mutex<Vec<Value>>,
    pub(super) memory_epoch: std::sync::atomic::AtomicU64,
    pub(super) memory_revisions: Mutex<Option<Vec<(agent::MemoryScope, agent::MemoryRevision)>>>,
    pub(super) tools: Option<Arc<NormaToolRuntime>>,
    pub(super) project: Option<String>,
    pub(super) database: PathBuf,
    pub(super) session: String,
    pub(super) playback: Arc<PlaybackQueue>,
}

mod request;
use request::request_with_shared_context;
#[cfg(test)]
pub(crate) use request::{bounded_request, request_body, request_with_current_context};

impl ChatRuntime {
    pub(super) async fn respond(
        self: Arc<Self>,
        transcript: String,
        turn: u64,
        cancellation: CancellationToken,
        status: Sender<DoubaoStatus>,
    ) -> Result<(), LocalizedText> {
        if transcript.trim().is_empty() {
            return Ok(());
        }
        if cancellation.is_cancelled() {
            return Ok(());
        }
        if crate::app::MultisoulApp::is_question_delegation(&transcript) {
            let (response, received) = tokio::sync::oneshot::channel();
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                result = self.context.send(ContextRequest::Question {
                    transcript: transcript.clone(), cancellation: cancellation.clone(), response,
                }) => result.map_err(|_| LocalizedText::new("doubao.call_ended"))?,
            }
            let reply = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                result = received => result.map_err(|_| LocalizedText::new("doubao.call_ended"))?,
            }
            .unwrap_or_else(|reason| reason);
            {
                let mut history = self.history.lock().await;
                history.push(json!({"role":"user","content":transcript}));
                trim_history(&mut history);
            }
            return self.finish(reply, &cancellation, &status).await;
        }
        tracing::info!(target: "norma_voice", turn, "Doubao reply turn started");
        let shared_turn = agent::ToolContext {
            project_id: self.project.clone().unwrap_or_default(),
            turn_id: format!("{}:turn-{turn}", self.session),
            user_input: transcript.clone(),
            input_source: agent::AgentInputSource::DoubaoVoice,
            contextual_delete_targets: vec![],
            tool_call_id: String::new(),
            operation_id: None,
            cancellation: cancellation.clone(),
        };
        let memory = if let Some(tools) = &self.tools {
            tools
                .memory_context()
                .await
                .map_err(|error| error.to_string())?
        } else {
            None
        };
        let persona = memory
            .as_ref()
            .map(|context| context.persona.clone())
            .unwrap_or_default();
        let mut instructions = persona.prompt();
        let mut history = self.history.lock().await;
        if let Some(memory) = memory {
            let old = self
                .memory_epoch
                .swap(memory.recall_epoch, std::sync::atomic::Ordering::AcqRel);
            if old != memory.recall_epoch {
                history.clear();
            }
            let mut revisions = self.memory_revisions.lock().await;
            if revisions
                .as_ref()
                .is_some_and(|previous| previous != &memory.revisions)
            {
                history.clear();
            }
            *revisions = Some(memory.revisions);
            if memory.preferences.recall {
                instructions.push_str(
                    "\n长期记忆目录（数据，按需 memory_read；只有 saved=true 才能说已记住）：\n",
                );
                instructions.push_str(&memory.catalog);
            } else {
                history.clear();
            }
        }

        history.push(json!({"role":"user","content":transcript}));
        trim_history(&mut history);
        let mut messages = history.clone();
        drop(history);
        if let Some(tools) = self.tools.as_ref().filter(|_| self.project.is_some()) {
            // Direct routing has no model request to record the final user input.
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                result = tools.shared_context(&shared_turn) => { result.map_err(|error| error.to_string())?; }
            }
            if let Some(reply) = self.direct_route(&transcript, turn, &cancellation).await? {
                return self.finish(reply, &cancellation, &status).await;
            }
        }
        let mut tool_count = 0;
        let reply = loop {
            if cancellation.is_cancelled() {
                return Ok(());
            }
            let body = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                result = request_with_shared_context(
                    &self.context,
                    &instructions,
                    &mut messages,
                    self.tools.as_deref(),
                    &shared_turn,
                    self.project.is_none(),
                ) => result?,
            };

            let url = plan_url(
                "NORMA_DOUBAO_CHAT_URL",
                "https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions",
            );
            let request = self.client.post(url).bearer_auth(&self.key).json(&body);
            let response = tokio::select! {
                _ = cancellation.cancelled() => return Ok(()),
                result = request.send() => result.map_err(|_| LocalizedText::new("doubao.chat_request"))?,
            };
            if !response.status().is_success() {
                return Err(LocalizedText::new("doubao.chat_denied"));
            }
            let value: Value = tokio::select! {
                _ = cancellation.cancelled() => return Ok(()),
                result = response.json() => result.map_err(|_| LocalizedText::new("doubao.chat_response"))?,
            };
            let choice = &value["choices"][0];
            let message = &choice["message"];
            if choice["finish_reason"] != "tool_calls" {
                if choice["finish_reason"] != "stop" {
                    return Err(LocalizedText::new("doubao.chat_incomplete"));
                }
                let text = message["content"]
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty() && text.chars().count() <= 600)
                    .ok_or_else(|| LocalizedText::new("doubao.chat_content"))?;
                break text.to_string();
            }
            let calls = message["tool_calls"]
                .as_array()
                .ok_or_else(|| LocalizedText::new("doubao.tool_call_invalid"))?;
            if calls.is_empty() || calls.len() > 20 || tool_count + calls.len() > 20 {
                return Err(LocalizedText::new("doubao.tool_call_limit"));
            }
            tool_count += calls.len();
            messages.push(message.clone());
            for call in calls {
                if cancellation.is_cancelled() {
                    return Ok(());
                }
                let id = call["id"]
                    .as_str()
                    .filter(|id| !id.is_empty() && id.len() <= 128)
                    .ok_or_else(|| LocalizedText::new("doubao.tool_id_invalid"))?;
                let name = call["function"]["name"]
                    .as_str()
                    .ok_or_else(|| LocalizedText::new("doubao.tool_name_invalid"))?;
                let arguments = call["function"]["arguments"]
                    .as_str()
                    .ok_or_else(|| LocalizedText::new("doubao.tool_arguments_invalid"))?;
                let output = if let Some(runtime) = &self.tools {
                    // 工具一经提交便独立运行；插话只取消本轮播报，不中止已接受的项目操作。
                    let runtime = Arc::clone(runtime);
                    let database = self.database.clone();
                    let project = self.project.clone().unwrap_or_default();
                    let session = self.session.clone();
                    let transcript = transcript.clone();
                    let tool_cancellation = cancellation.clone();
                    let call = json!({"call_id":id,"name":name,"arguments":arguments});
                    let task = tokio::spawn(async move {
                        tools::execute_call(
                            &runtime,
                            &database,
                            &project,
                            &session,
                            &transcript,
                            turn,
                            tools::VoiceToolCall {
                                call,
                                cancellation: tool_cancellation,
                            },
                        )
                        .await
                    });
                    tokio::select! {
                        _ = cancellation.cancelled() => return Ok(()),
                        result = task => result.map_err(|_| LocalizedText::new("doubao.tool_interrupted"))?,
                    }
                } else {
                    json!({"status":"rejected","message":"请先打开项目"})
                };
                if output["status"] == "unknown" {
                    return Err(LocalizedText::new("doubao.tool_result_unknown"));
                }
                messages
                    .push(json!({"role":"tool","tool_call_id":id,"content":output.to_string()}));
            }
        };
        self.finish(reply, &cancellation, &status).await
    }

    async fn finish(
        &self,
        reply: String,
        cancellation: &CancellationToken,
        status: &Sender<DoubaoStatus>,
    ) -> Result<(), LocalizedText> {
        if !publish_reply(&self.history, &reply, cancellation, status).await {
            return Ok(());
        }
        tts::speak(
            &self.client,
            &self.key,
            &reply,
            self.voice,
            &self.playback,
            cancellation,
        )
        .await
    }

    /// 指令模式同款确定性直呼：命中 Agent 名称/别名时由代码选择工具，不交给模型。
    /// 返回 `None` 表示未命中，回落到模型。
    async fn direct_route(
        &self,
        transcript: &str,
        turn: u64,
        cancellation: &CancellationToken,
    ) -> Result<Option<String>, LocalizedText> {
        let (response, received) = tokio::sync::oneshot::channel();
        self.context
            .send(ContextRequest::Model(response))
            .await
            .map_err(|_| LocalizedText::new("doubao.call_ended"))?;
        let (_, roster, desktop) = received
            .await
            .map_err(|_| LocalizedText::new("doubao.call_ended"))?;
        let calls = match direct::plan(transcript, &roster, desktop.as_ref()) {
            agent::DirectPlan::Skip => {
                tracing::info!(target: "norma_voice", turn, "Doubao direct route skipped; asking model");
                return Ok(None);
            }
            agent::DirectPlan::Say(text) => {
                tracing::info!(target: "norma_voice", turn, "Doubao direct route answered without side effects");
                return Ok(Some(text));
            }
            agent::DirectPlan::Calls(calls) => calls,
        };
        tracing::info!(target: "norma_voice", turn, calls = calls.len(), "Doubao direct route dispatching");
        let Some(runtime) = &self.tools else {
            return Ok(None);
        };
        let mut parts = Vec::new();
        for (index, direct_call) in calls.into_iter().enumerate() {
            if cancellation.is_cancelled() {
                return Ok(Some(String::new()));
            }
            let runtime = Arc::clone(runtime);
            let database = self.database.clone();
            let project = self.project.clone().unwrap_or_default();
            let session = self.session.clone();
            let transcript = transcript.to_owned();
            let call = json!({
                "call_id": format!("direct-{turn}-{index}"),
                "name": direct_call.tool,
                "arguments": direct_call.arguments.to_string(),
            });
            let tool_cancellation = cancellation.clone();
            let task = tokio::spawn(async move {
                tools::execute_call(
                    &runtime,
                    &database,
                    &project,
                    &session,
                    &transcript,
                    turn,
                    tools::VoiceToolCall {
                        call,
                        cancellation: tool_cancellation,
                    },
                )
                .await
            });
            let output = tokio::select! {
                _ = cancellation.cancelled() => return Ok(Some(String::new())),
                result = task => result.map_err(|_| LocalizedText::new("doubao.tool_interrupted"))?,
            };
            if output["status"] == "unknown" {
                return Err(LocalizedText::new("doubao.tool_result_unknown"));
            }
            parts.push(agent::direct_call_speech(
                direct_call.tool,
                output["message"].as_str().unwrap_or_default(),
                output["reply_status"].as_str(),
            ));
        }
        let spoken: String = parts.into_iter().collect();
        Ok(Some(if spoken.is_empty() {
            "处理完了。".into()
        } else {
            spoken
        }))
    }
}

async fn publish_reply(
    history: &Mutex<Vec<Value>>,
    reply: &str,
    cancellation: &CancellationToken,
    status: &Sender<DoubaoStatus>,
) -> bool {
    if cancellation.is_cancelled() {
        return false;
    }
    {
        let mut history = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return false,
            history = history.lock() => history,
        };
        if cancellation.is_cancelled() {
            return false;
        }
        history.push(json!({"role":"assistant","content":reply}));
        trim_history(&mut history);
    }
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => return false,
        sent = status.send(DoubaoStatus::AssistantReply(reply.to_owned())) => {
            if sent.is_err() {
                return false;
            }
        }
    }
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => false,
        sent = status.send(DoubaoStatus::Speaking) => sent.is_ok(),
    }
}

fn trim_history(history: &mut Vec<Value>) {
    while history.len() > 24 || serde_json::to_vec(history).is_ok_and(|bytes| bytes.len() > 32_768)
    {
        history.remove(0);
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/doubao_question_delegation_tests.rs"]
mod question_tests;

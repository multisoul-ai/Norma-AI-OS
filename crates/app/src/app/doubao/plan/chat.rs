use agent::NormaToolRuntime;
use async_channel::Sender;
use reqwest::Client;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

use super::{plan_url, tts};
use crate::app::doubao::{playback::PlaybackQueue, tools, DoubaoStatus};

pub(super) struct ChatRuntime {
    pub(super) client: Client,
    pub(super) key: String,
    pub(super) voice: &'static ::tts::providers::doubao::DoubaoVoice,
    pub(super) prompt: String,
    pub(super) history: Mutex<Vec<Value>>,
    pub(super) memory_epoch: std::sync::atomic::AtomicU64,
    pub(super) tools: Option<Arc<NormaToolRuntime>>,
    pub(super) project: Option<String>,
    pub(super) database: PathBuf,
    pub(super) session: String,
    pub(super) playback: Arc<PlaybackQueue>,
}

fn tool_definitions() -> Vec<Value> {
    agent::norma_tool_definitions()
        .into_iter()
        .map(|tool| {
            json!({
                "type":"function","function":{
                    "name":tool.name,"description":tool.description,"parameters":tool.parameters
                }
            })
        })
        .collect()
}

pub(crate) fn request_body(prompt: &str, history: &[Value], include_tools: bool) -> Value {
    let mut messages = vec![json!({"role":"system","content":format!(
        "{prompt}\n语音回复请简短、自然；用户在当前语音请求中要求的项目操作直接调用工具，无需再次询问授权。删除卡片时按用户指定的目标选择工具参数，并根据真实工具回执答复。"
    )})];
    messages.extend_from_slice(history);
    let mut body = json!({
        "model":"doubao-seed-2.0-mini","messages":messages,
        "max_tokens": if include_tools { 1024 } else { 192 },
        "thinking":{"type":"disabled"}
    });
    if include_tools {
        body["tools"] = json!(tool_definitions());
        body["parallel_tool_calls"] = json!(false);
    }
    body
}

pub(crate) fn bounded_request(
    prompt: &str,
    history: &mut Vec<Value>,
    include_tools: bool,
    personal: bool,
) -> Result<Value, LocalizedText> {
    // Use a conservative 16K-token context ceiling for the fixed voice model.
    // Serialized 2 bytes/token; reserve reply(1024 tokens) + memory tools(8192 bytes).
    let budget = 16_384 * 2 - 1024 * 2 - agent::MEMORY_TOOL_BYTES;
    loop {
        let mut body = request_body(prompt, history, include_tools);
        if personal {
            if let Some(tools) = body["tools"].as_array_mut() {
                tools.retain(|tool| {
                    tool["function"]["name"]
                        .as_str()
                        .is_some_and(|name| name.starts_with("memory_"))
                });
            }
        }
        if body.to_string().len() <= budget {
            return Ok(body);
        }
        // Remove a whole earlier user turn, retaining assistant/tool pairs together.
        let next = history
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, message)| message["role"] == "user")
            .map(|(index, _)| index);
        if let Some(next) = next {
            history.drain(..next);
        } else {
            return Err(LocalizedText::new("doubao.context_budget"));
        }
    }
}

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
        let mut prompt = self.prompt.clone();
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
        prompt.push_str(&persona.prompt());
        let mut history = self.history.lock().await;
        if let Some(memory) = memory {
            let old = self
                .memory_epoch
                .swap(memory.recall_epoch, std::sync::atomic::Ordering::AcqRel);
            if old != memory.recall_epoch {
                history.clear();
            }
            if memory.preferences.recall {
                prompt.push_str(
                    "\n长期记忆目录（数据，按需 memory_read；只有 saved=true 才能说已记住）：\n",
                );
                prompt.push_str(&memory.catalog);
            } else {
                history.clear();
            }
        }

        history.push(json!({"role":"user","content":transcript}));
        trim_history(&mut history);
        let mut messages = history.clone();
        drop(history);
        let mut tool_count = 0;
        let reply = loop {
            if cancellation.is_cancelled() {
                return Ok(());
            }
            let body = bounded_request(
                &prompt,
                &mut messages,
                self.tools.is_some(),
                self.project.is_none(),
            )?;

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
        if cancellation.is_cancelled() {
            return Ok(());
        }
        {
            let mut history = self.history.lock().await;
            history.push(json!({"role":"assistant","content":reply}));
            trim_history(&mut history);
        }
        let _ = status.send(DoubaoStatus::Speaking).await;
        tts::speak(
            &self.client,
            &self.key,
            &reply,
            self.voice,
            &self.playback,
            &cancellation,
        )
        .await
    }
}

fn trim_history(history: &mut Vec<Value>) {
    while history.len() > 24 || serde_json::to_vec(history).is_ok_and(|bytes| bytes.len() > 32_768)
    {
        history.remove(0);
    }
}

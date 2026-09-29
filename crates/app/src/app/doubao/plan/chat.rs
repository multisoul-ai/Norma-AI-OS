use agent::NormaToolRuntime;
use reqwest::Client;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::{plan_url, tts};
use crate::app::doubao::{playback::PlaybackQueue, tools};

pub(super) struct ChatRuntime {
    pub(super) client: Client,
    pub(super) key: String,
    pub(super) prompt: String,
    pub(super) history: Mutex<Vec<Value>>,
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
        "{prompt}\n语音回复请简短、自然；项目操作必须依照现有工具规则和用户明确授权。"
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

impl ChatRuntime {
    pub(super) async fn respond(
        self: Arc<Self>,
        transcript: String,
        turn: u64,
        cancellation: CancellationToken,
    ) -> Result<(), String> {
        if transcript.trim().is_empty() {
            return Ok(());
        }
        let mut history = self.history.lock().await;
        history.push(json!({"role":"user","content":transcript}));
        trim_history(&mut history);
        let mut messages = history.clone();
        drop(history);
        let mut tool_count = 0;
        let reply = loop {
            if cancellation.is_cancelled() {
                return Ok(());
            }
            let body = request_body(&self.prompt, &messages, self.tools.is_some());
            let url = plan_url(
                "NORMA_DOUBAO_CHAT_URL",
                "https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions",
            );
            let request = self.client.post(url).bearer_auth(&self.key).json(&body);
            let response = tokio::select! {
                _ = cancellation.cancelled() => return Ok(()),
                result = request.send() => result.map_err(|_| "方舟对话请求失败".to_string())?,
            };
            if !response.status().is_success() {
                return Err("方舟对话模型拒绝了请求；请检查 API Key 与模型权限".into());
            }
            let value: Value = tokio::select! {
                _ = cancellation.cancelled() => return Ok(()),
                result = response.json() => result.map_err(|_| "方舟对话响应无效".to_string())?,
            };
            let choice = &value["choices"][0];
            let message = &choice["message"];
            if choice["finish_reason"] != "tool_calls" {
                if choice["finish_reason"] != "stop" {
                    return Err("方舟对话未完成".into());
                }
                let text = message["content"]
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty() && text.chars().count() <= 600)
                    .ok_or_else(|| "方舟回复内容无效".to_string())?;
                break text.to_string();
            }
            let calls = message["tool_calls"]
                .as_array()
                .ok_or_else(|| "方舟工具调用无效".to_string())?;
            if calls.is_empty() || calls.len() > 20 || tool_count + calls.len() > 20 {
                return Err("方舟工具调用超出上限".into());
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
                    .ok_or_else(|| "方舟工具调用 ID 无效".to_string())?;
                let name = call["function"]["name"]
                    .as_str()
                    .ok_or_else(|| "方舟工具名无效".to_string())?;
                let arguments = call["function"]["arguments"]
                    .as_str()
                    .ok_or_else(|| "方舟工具参数无效".to_string())?;
                let output = if let (Some(runtime), Some(project)) = (&self.tools, &self.project) {
                    // 工具一经提交便独立运行；插话只取消本轮播报，不中止已接受的项目操作。
                    let runtime = Arc::clone(runtime);
                    let database = self.database.clone();
                    let project = project.clone();
                    let session = self.session.clone();
                    let transcript = transcript.clone();
                    let call = json!({"call_id":id,"name":name,"arguments":arguments});
                    let task = tokio::spawn(async move {
                        tools::execute_call(
                            &runtime,
                            &database,
                            &project,
                            &session,
                            &transcript,
                            turn,
                            call,
                        )
                        .await
                    });
                    tokio::select! {
                        _ = cancellation.cancelled() => return Ok(()),
                        result = task => result.map_err(|_| "项目工具执行中断".to_string())?,
                    }
                } else {
                    json!({"status":"rejected","message":"请先打开项目"})
                };
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
        tts::speak(
            &self.client,
            &self.key,
            &reply,
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

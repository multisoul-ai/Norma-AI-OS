use agent::{AdapterResult, NormaToolRuntime, ToolContext};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::OnceLock};
use storage::{MobileCommandReservation, MobileCommandStore, NewMobileCommand};
use tokio_util::sync::CancellationToken;

fn hash(parts: &[&str]) -> String {
    let encoded = serde_json::to_vec(parts).expect("string slices are serializable");
    hex::encode(Sha256::digest(encoded))
}

fn owner_id() -> &'static str {
    static OWNER: OnceLock<String> = OnceLock::new();
    OWNER.get_or_init(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        format!("desktop-{}-{nanos}", std::process::id())
    })
}

fn unknown() -> AdapterResult {
    AdapterResult::unknown("执行结果待核对；不会重复派工")
}

pub(super) struct VoiceToolCall {
    pub call: Value,
    pub cancellation: CancellationToken,
}

pub(super) async fn execute_call(
    runtime: &NormaToolRuntime,
    database: &Path,
    project: &str,
    session: &str,
    transcript: &str,
    turn: u64,
    request_call: VoiceToolCall,
) -> Value {
    let VoiceToolCall { call, cancellation } = request_call;
    let Some(call_id) = call["call_id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 128)
    else {
        return json!({"status":"rejected","message":"工具调用缺少 ID"});
    };
    let Some(name) = call["name"].as_str() else {
        return json!({"status":"rejected","message":"工具名缺失"});
    };
    let arguments = match call["arguments"]
        .as_str()
        .map(serde_json::from_str::<Value>)
    {
        Some(Ok(value)) if value.is_object() => value,
        _ => return json!({"status":"rejected","message":"工具参数无效"}),
    };
    if transcript.trim().is_empty() {
        return json!({"status":"rejected","message":"缺少用户实际转写，不能执行工具"});
    }
    let operation_id = format!("db-{}", hash(&[project, session, call_id]));
    let request = NewMobileCommand {
        device_id: "desktop-doubao".into(),
        operation_id: operation_id.clone(),
        workspace_id: format!("ws-{}", hash(&[project])),
        route_id: format!("session-{}", hash(&[session])),
        owner_id: owner_id().into(),
        request_hash: hash(&[name, &arguments.to_string(), transcript]),
    };
    let store = MobileCommandStore::new(database.to_path_buf());
    let reservation = tokio::task::spawn_blocking({
        let store = store.clone();
        let request = request.clone();
        move || store.reserve(&request)
    })
    .await;
    match reservation {
        Ok(Ok(MobileCommandReservation::New)) => {}
        Ok(Ok(MobileCommandReservation::Existing(receipt))) => {
            let result = match receipt.state.as_str() {
                "succeeded" => receipt
                    .result
                    .and_then(|value| serde_json::from_value::<AdapterResult>(value).ok())
                    .map(|result| json!(result))
                    .unwrap_or_else(|| json!(unknown())),
                "rejected" => json!({"status":"rejected","message":"原工具调用已拒绝，未再次执行"}),
                _ => json!(unknown()),
            };
            return result;
        }
        _ => return json!(unknown()),
    }
    let context = ToolContext {
        project_id: project.into(),
        turn_id: format!("{session}:turn-{turn}"),
        user_input: transcript.into(),
        input_source: agent::AgentInputSource::DoubaoVoice,
        contextual_delete_targets: Vec::new(),
        tool_call_id: call_id.into(),
        operation_id: Some(operation_id),
        // 插话或结束通话后，已提交的项目任务仍可完成。
        cancellation: if name.starts_with("memory_") {
            cancellation
        } else {
            CancellationToken::new()
        },
    };
    let executed = runtime.execute(name, arguments, &context).await;
    let (result, durable_outcome) = match executed {
        Ok(receipt) => (json!(receipt), Ok(json!(receipt))),
        Err(error) => (
            json!({"status":"rejected","message":error.to_string()}),
            Err("rejected".into()),
        ),
    };
    let persisted =
        tokio::task::spawn_blocking(move || store.finish(&request, durable_outcome)).await;
    if !matches!(persisted, Ok(Ok(()))) {
        return json!(unknown());
    }
    result
}

use super::*;
use agent::context::{ContextAssemblyError, ContextConsumer, ContextFrame, SHARED_CONTEXT_RULES};

pub(super) async fn request_with_shared_context(
    context: &ContextRequests,
    instructions: &str,
    history: &mut Vec<Value>,
    tools: Option<&NormaToolRuntime>,
    turn: &agent::ToolContext,
    personal: bool,
) -> Result<Value, LocalizedText> {
    let body =
        request_with_current_context(context, instructions, history, tools.is_some(), personal)
            .await?;
    // The adapter's exact-scope checkpoint must be the last await before assembly.
    // An overview/workspace switch while waiting for Model invalidates that source.
    let frame = if let Some(tools) = tools {
        tools
            .shared_context(turn)
            .await
            .map_err(|error| error.to_string())?
    } else {
        None
    };
    match frame {
        Some(frame) => insert_shared_data(body, history, &frame, turn),
        None => Ok(body),
    }
}

/// Measure the actual JSON including the data message's outer escaping. Keep the
/// required state and current turn intact; only complete older turns may be removed.
fn insert_shared_data(
    body: Value,
    history: &mut Vec<Value>,
    frame: &ContextFrame,
    turn: &agent::ToolContext,
) -> Result<Value, LocalizedText> {
    let system = json!({"role":"system", "content": format!("{}{}",
        body["messages"][0]["content"].as_str().unwrap_or_default(), SHARED_CONTEXT_RULES)});
    loop {
        let mut messages = vec![system.clone()];
        messages.extend_from_slice(history);
        let with_data = |data: &str| {
            let mut request = body.clone();
            let mut request_messages = messages.clone();
            request_messages.insert(1, json!({"role":"user","content":data}));
            request["messages"] = json!(request_messages);
            request
        };
        match agent::context::render_context_frame_measured(
            frame,
            ContextConsumer::Doubao,
            &turn.turn_id,
            &turn.user_input,
            REQUEST_INPUT_BYTES,
            |data| with_data(data).to_string().len(),
        ) {
            Ok(data) => return Ok(with_data(&data)),
            Err(ContextAssemblyError::BudgetExceeded { .. }) => {}
            Err(error) => return Err(error.to_string().into()),
        }
        if !remove_oldest_turn(history) {
            return Err(LocalizedText::new("doubao.context_budget"));
        }
    }
}

fn remove_oldest_turn(history: &mut Vec<Value>) -> bool {
    let next = history
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, message)| message["role"] == "user")
        .map(|(index, _)| index);
    if let Some(next) = next {
        history.drain(..next);
        true
    } else {
        false
    }
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
        "model":CHAT_MODEL,"messages":messages,
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
        if body.to_string().len() <= REQUEST_INPUT_BYTES {
            return Ok(body);
        }
        // Remove a whole earlier user turn, retaining assistant/tool pairs together.
        if !remove_oldest_turn(history) {
            return Err(LocalizedText::new("doubao.context_budget"));
        }
    }
}

pub(crate) async fn request_with_current_context(
    context: &ContextRequests,
    instructions: &str,
    history: &mut Vec<Value>,
    include_tools: bool,
    personal: bool,
) -> Result<Value, LocalizedText> {
    let (response, received) = tokio::sync::oneshot::channel();
    context
        .send(ContextRequest::Model(response))
        .await
        .map_err(|_| LocalizedText::new("doubao.call_ended"))?;
    let (canvas, roster, desktop) = received
        .await
        .map_err(|_| LocalizedText::new("doubao.call_ended"))?;
    let mut prompt =
        agent::norma_system_prompt_base_with_desktop(&canvas, &roster, desktop.as_ref());
    prompt.push_str(instructions);
    let desktop_active = direct::desktop_active(desktop.as_ref());
    let mut body = bounded_request(&prompt, history, include_tools, personal && !desktop_active)?;
    if personal && desktop_active {
        if let Some(tools) = body["tools"].as_array_mut() {
            tools.retain(|tool| {
                tool["function"]["name"].as_str().is_some_and(|name| {
                    name.starts_with("memory_")
                        || matches!(
                            name,
                            "workspace_snapshot"
                                | "assign_workspace_task"
                                | "open_workspace_artifact"
                                | "locate_workspace_agent"
                                | "create_browser"
                                | "create_preview"
                        )
                })
            });
        }
    }
    Ok(body)
}

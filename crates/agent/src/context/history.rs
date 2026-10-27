use super::*;
use std::collections::BTreeSet;

pub(super) fn validate(request: &AssemblyRequest) -> Result<(), ContextAssemblyError> {
    let invalid = || ContextAssemblyError::Invalid("native_input_or_history");
    match (&request.consumer, &request.input) {
        (ContextConsumer::LiveConversation, ContextInput::NativeAudio { session_id, .. })
            if !session_id.trim().is_empty() && request.history.is_empty() =>
        {
            return Ok(())
        }
        (ContextConsumer::LiveConversation, _) | (_, ContextInput::NativeAudio { .. }) => {
            return Err(invalid())
        }
        (_, ContextInput::Text { .. }) => {}
    }
    let mut required_seen = false;
    let mut turns = BTreeSet::new();
    for turn in &request.history {
        if turn.turn_id.trim().is_empty()
            || !turns.insert(&turn.turn_id)
            || (required_seen && !turn.required)
        {
            return Err(invalid());
        }
        required_seen |= turn.required;
        let mut pending = BTreeSet::new();
        let mut calls_seen = BTreeSet::new();
        if turn.messages.first().and_then(|item| item["role"].as_str()) != Some("user") {
            return Err(invalid());
        }
        for (index, message) in turn.messages.iter().enumerate() {
            match message["role"].as_str() {
                Some("user") if index == 0 => {}
                Some("assistant") if pending.is_empty() => {
                    if let Some(calls) = message.get("tool_calls") {
                        let calls = calls.as_array().ok_or_else(invalid)?;
                        if calls.is_empty() {
                            return Err(invalid());
                        }
                        for call in calls {
                            let id = call["id"]
                                .as_str()
                                .filter(|id| !id.is_empty())
                                .ok_or_else(invalid)?;
                            if !calls_seen.insert(id) {
                                return Err(invalid());
                            }
                            pending.insert(id);
                        }
                    }
                }
                Some("tool") => {
                    let id = message["tool_call_id"].as_str().ok_or_else(invalid)?;
                    if !pending.remove(id) {
                        return Err(invalid());
                    }
                }
                _ => return Err(invalid()),
            }
        }
        if !pending.is_empty() {
            return Err(invalid());
        }
    }
    Ok(())
}

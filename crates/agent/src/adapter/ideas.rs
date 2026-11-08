use super::{AdapterCommand, AdapterError};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureIdeaArgs {
    body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

pub(super) fn parse_idea_command(
    name: &str,
    arguments: Value,
) -> Option<Result<AdapterCommand, AdapterError>> {
    Some(match name {
        "idea_save" => capture(arguments),
        "idea_list" => parse::<EmptyArgs>(arguments).map(|_| AdapterCommand::ListIdeas),
        _ => return None,
    })
}

fn capture(arguments: Value) -> Result<AdapterCommand, AdapterError> {
    let args: CaptureIdeaArgs = parse(arguments)?;
    Ok(AdapterCommand::CaptureIdea {
        body: text("body", args.body)?,
    })
}

fn parse<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, AdapterError> {
    serde_json::from_value(value).map_err(|error| invalid(&error.to_string()))
}

fn text(field: &str, value: String) -> Result<String, AdapterError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(invalid(&format!("{field} must not be empty")));
    }
    Ok(trimmed.to_string())
}

fn invalid(message: &str) -> AdapterError {
    AdapterError::InvalidArguments(message.to_string())
}

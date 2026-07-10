use std::collections::VecDeque;

use crate::{ChatRequest, ChatResponse, LlmClient, LlmError, ToolCall};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MockStep {
    Text(String),
    Tools(Vec<ToolCall>),
    TextAndTools { text: String, tools: Vec<ToolCall> },
}

pub struct MockLlmClient {
    steps: VecDeque<MockStep>,
}

impl MockLlmClient {
    pub fn script(steps: Vec<MockStep>) -> Self {
        Self {
            steps: steps.into(),
        }
    }
}

impl LlmClient for MockLlmClient {
    fn chat(&mut self, _request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let step = self
            .steps
            .pop_front()
            .ok_or_else(|| LlmError::Message("mock exhausted".into()))?;

        let response = match step {
            MockStep::Text(text) => ChatResponse {
                assistant_text: text,
                tool_calls: vec![],
            },
            MockStep::Tools(tools) => ChatResponse {
                assistant_text: String::new(),
                tool_calls: tools,
            },
            MockStep::TextAndTools { text, tools } => ChatResponse {
                assistant_text: text,
                tool_calls: tools,
            },
        };

        Ok(response)
    }
}

use super::DeleteCanvasCardTool;
use crate::{AdapterError, AdapterResult};
use rig_core::tool::{Tool, ToolCallExtensions};
use serde_json::Value;

#[derive(Clone)]
pub(crate) struct GptLiveDeleteCanvasCardTool(pub DeleteCanvasCardTool);

impl Tool for GptLiveDeleteCanvasCardTool {
    const NAME: &'static str = DeleteCanvasCardTool::NAME;
    type Error = AdapterError;
    type Args = Value;
    type Output = AdapterResult;

    fn description(&self) -> String {
        self.0.description()
    }

    fn parameters(&self) -> Value {
        let mut parameters = self.0.parameters();
        // OpenAI rejects root oneOf; the original executor still enforces exactly one target.
        parameters
            .as_object_mut()
            .expect("delete_canvas_card has an object schema")
            .remove("oneOf");
        parameters
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.0.call(args).await
    }

    async fn call_with_extensions(
        &self,
        args: Self::Args,
        extensions: &ToolCallExtensions,
    ) -> Result<Self::Output, Self::Error> {
        self.0.call_with_extensions(args, extensions).await
    }
}

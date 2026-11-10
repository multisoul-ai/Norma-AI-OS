use super::summary::bounded_excerpt;
use norma::mobile::RecentTool;
use serde_json::Value;

/// Tool name plus the provider's human description, or the basename of the file it touches.
/// Command bodies, directories, file contents and results never leave the hook payload.
pub(super) fn recent_tool(raw: &Value) -> Option<RecentTool> {
    let name = bounded_excerpt(raw["tool_name"].as_str()?)?;
    let input = &raw["tool_input"];
    let arg = input["description"]
        .as_str()
        .and_then(bounded_excerpt)
        .or_else(|| {
            ["file_path", "path", "notebook_path"]
                .iter()
                .find_map(|key| input[*key].as_str())
                .and_then(|path| path.rsplit('/').next())
                .and_then(bounded_excerpt)
        });
    Some(RecentTool { name, arg })
}

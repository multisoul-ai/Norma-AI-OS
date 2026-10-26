use super::*;

/// Only prepare the explicitly addressed project. Authentication and exact card
/// identity checks remain in the caller after this asynchronous preparation.
pub(crate) async fn prepare_scoped(
    database: &Path,
    areas: &[WorkspaceArea],
    command: &AdapterCommand,
    cx: &mut gpui::AsyncApp,
) -> Result<(), AdapterError> {
    let address = match command {
        AdapterCommand::AssignAgentTask { agent, .. }
        | AdapterCommand::ReplyAgent { agent, .. } => agent,
        _ => return Ok(()),
    };
    let (area, _) = address
        .split_once('/')
        .ok_or_else(|| AdapterError::Rejected("必须使用 项目区域ID/卡片ID".into()))?;
    prepare_area(database, areas, area, cx).await
}

pub(super) async fn prepare_area(
    database: &Path,
    areas: &[WorkspaceArea],
    area_id: &str,
    cx: &mut gpui::AsyncApp,
) -> Result<(), AdapterError> {
    let area = areas
        .iter()
        .find(|bound| bound.area.id == area_id)
        .ok_or_else(|| AdapterError::Rejected("项目不在群聊内".into()))?;
    let service = cx
        .update(|cx| {
            let session = crate::workspace::desktop::mobile::session(database, &area.area, cx)
                .ok_or_else(|| AdapterError::Rejected("项目不可用".into()))?;
            session.update(cx, |app, _| app.shared_context_service())
        })
        .map_err(|error| AdapterError::Failed(error.to_string()))??;
    service.snapshot().await?;
    Ok(())
}

/// The desktop location visible to the user when a Norma turn starts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DesktopContext {
    NoWorkspace,
    WorkspaceOverview {
        workspace_id: String,
        workspace_name: String,
    },
    ProjectCanvas {
        workspace_id: String,
        workspace_name: String,
        area_id: String,
        project_name: String,
    },
    Switching {
        workspace_id: String,
        workspace_name: String,
    },
}

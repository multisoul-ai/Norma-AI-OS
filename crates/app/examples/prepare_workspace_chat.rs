//! Prepare disposable local data through official stores. Never opens a user's database.
use anyhow::{ensure, Result};
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, Document};
use norma::mobile::ServerIdentity;
use serde_json::json;
use std::path::PathBuf;
use storage::{AppStore, DatabasePaths, MobilePermission, NewAgentDefinition, ProjectStore};

fn main() -> Result<()> {
    let root = PathBuf::from(std::env::var("NORMA_WORKSPACE_ACCEPTANCE_ROOT")?);
    ensure!(
        root.is_absolute() && root.ends_with("mac-data"),
        "explicit disposable root required"
    );
    ensure!(
        !root.join("app.sqlite").exists(),
        "refuse to replace an existing database"
    );
    std::fs::create_dir_all(&root)?;
    let paths = DatabasePaths::isolated_test(&root);
    let app = AppStore::initialize(&paths)?;
    let identity = ServerIdentity::generate()?;
    app.settings_store()
        .set_string("mobile.server_identity", &identity.storage_json()?)?;
    let spaces = app.desktop_workspace_store();
    let workspace = spaces.create("移动端体验")?;
    let other = spaces.create("发布质量")?;
    let project = root.parent().unwrap().join("acceptance-project");
    std::fs::create_dir_all(&project)?;
    std::fs::write(
        project.join("README.md"),
        "# Disposable workspace chat acceptance\n",
    )?;
    spaces.add_project(&workspace.id, &project)?;
    let area = spaces.list_projects(&workspace.id)?.remove(0);
    let definitions = ProjectStore::initialize_for_desktop_definitions(&paths, &area.project_key)?;
    let definition = definitions
        .agent_definition_store()
        .create(&NewAgentDefinition {
            name: "开发".into(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "仅操作本验收目录".into(),
        })?;
    let canvas =
        ProjectStore::initialize_for_desktop_area(&paths, &area.root, &workspace.id, &area.id)?;
    let mut document = Document::new();
    let node = document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("开发".into()),
            definition_id: Some(definition.id),
            mobile_control: false,
        }),
        CanvasRect::new(0., 0., 640., 400.),
    );
    canvas
        .canvas_store()
        .save_snapshot(&document.to_snapshot())?;
    app.workspace_identity_store()
        .resolve_or_create(&area.root, "iOS 验收项目")?;
    let device = uuid::Uuid::new_v4().to_string();
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    app.mobile_access_store().register_all_projects(
        &device,
        "本地模拟器验收",
        &token,
        MobilePermission::Control,
    )?;
    let fixture = json!({"machineID":identity.machine_id(),"certificateSHA256":identity.certificate_sha256(),"namespace":uuid::Uuid::new_v4().to_string(),"mac":{"deviceID":device,"token":token},"workspaceID":workspace.id,"otherWorkspaceID":other.id,"areaID":area.id,"definitionID":definition.id,"nodeID":node});
    std::fs::write(
        root.parent().unwrap().join("workspace-fixture.json"),
        serde_json::to_vec_pretty(&fixture)?,
    )?;
    println!("Prepared disposable workspaces and native Agent card; credentials saved privately in fixture.");
    Ok(())
}

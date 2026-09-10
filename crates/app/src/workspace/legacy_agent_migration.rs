use agent::{AgentName, AgentNameRegistry};
use canvas::CanvasNodeKind;
use runtime::Runtime;
use storage::{AgentDefinition, NewAgentDefinition, ProjectStore, StorageError};

pub(crate) fn migrate_legacy_agent_cards(
    runtime: &mut Runtime,
    project_store: &ProjectStore,
) -> Result<usize, String> {
    let legacy = runtime
        .document
        .nodes()
        .iter()
        .filter_map(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
                .is_none()
                .then(|| (node.id, spec.profile, spec.alias.clone()))
        })
        .collect::<Vec<_>>();
    if legacy.is_empty() {
        return Ok(0);
    }

    let definition_store = project_store.agent_definition_store();
    for (node_id, profile, legacy_alias) in &legacy {
        let definition =
            create_migrated_definition(&definition_store, *profile, legacy_alias.as_deref())?;
        let node = runtime
            .document
            .node_mut(*node_id)
            .ok_or_else(|| format!("Legacy Agent card #{node_id} disappeared during migration"))?;
        let CanvasNodeKind::Agent(spec) = &mut node.kind else {
            return Err(format!(
                "Legacy Agent card #{node_id} changed kind during migration"
            ));
        };
        spec.definition_id = Some(definition.id);
        spec.alias = Some(definition.name);
    }
    runtime
        .save_canvas_snapshot(&project_store.canvas_store())
        .map_err(|error| format!("Could not save migrated Agent cards: {error}"))?;
    Ok(legacy.len())
}

fn create_migrated_definition(
    store: &storage::AgentDefinitionStore,
    profile: canvas::AgentProfile,
    legacy_alias: Option<&str>,
) -> Result<AgentDefinition, String> {
    if let Some(alias) = legacy_alias.filter(|alias| AgentName::parse(alias).is_ok()) {
        match store.create(&NewAgentDefinition {
            name: alias.to_string(),
            runtime_profile: profile,
            workflow_description: String::new(),
        }) {
            Ok(definition) => return Ok(definition),
            Err(StorageError::AgentNameConflict { .. }) => {}
            Err(error) => return Err(format!("Could not migrate legacy Agent name: {error}")),
        }
    }

    let existing = store
        .list()
        .map_err(|error| format!("Could not list Agent names during migration: {error}"))?;
    let registry =
        AgentNameRegistry::from_names(existing.iter().map(|definition| &definition.name))
            .map_err(|error| format!("Could not build Agent name registry: {error}"))?;
    let builtin = registry.next_available_builtin().ok_or_else(|| {
        "All 30 built-in Agent names are in use; rename a legacy Agent card before migration"
            .to_string()
    })?;
    store
        .create(&NewAgentDefinition {
            name: builtin.name().to_string(),
            runtime_profile: profile,
            workflow_description: String::new(),
        })
        .map_err(|error| format!("Could not allocate a name for a legacy Agent card: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::{AgentProfile, AgentSpec, CanvasRect};

    /// 旧 alias-only Agent 卡片首次打开时应创建定义并回写稳定绑定，第二次迁移不得重复创建。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   legacy cards = 2：合法 alias="小安"、非法 alias="Code Reviewer"
    ///   existing definitions = 0
    ///   第一次迁移新增定义 = 2；第二次迁移新增定义 = 0
    ///   非法 alias 回退名称池首项 = Alan
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 建立临时 project store，并向 Runtime document 放入两张 definition_id=None 卡片
    ///   2. migrate_legacy_agent_cards → 保留小安，为非法 alias 分配 Alan，并保存 snapshot
    ///   3. 再次执行迁移并读取 definitions、nodes 与持久化 snapshot
    ///
    /// 预期结果：
    ///   - 正断言：第一次迁移 2 张、第二次 0 张，定义总数保持 2
    ///   - 正断言：两张卡片均有 definition_id，名称分别为小安与 Alan
    ///   - 负断言：非法旧名称 Code Reviewer 不应进入定义或快照
    #[test]
    fn legacy_agent_cards_migrate_once_and_persist_definition_identity() {
        let temp = tempfile::tempdir().expect("测试应创建临时目录");
        let workspace = temp.path().join("legacy-agent-workspace");
        std::fs::create_dir(&workspace).expect("测试应创建 workspace 目录");
        let project_store =
            ProjectStore::for_workspace(&workspace).expect("project store 应成功打开");
        project_store.migrate().expect("project store 应成功迁移");
        let mut runtime = Runtime::new();
        runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Claude,
                alias: Some("小安".to_string()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        );
        runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: Some("Code Reviewer".to_string()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(660.0, 0.0, 640.0, 400.0),
        );

        let first = migrate_legacy_agent_cards(&mut runtime, &project_store)
            .expect("首次打开应成功迁移两张旧 Agent 卡片");
        let second = migrate_legacy_agent_cards(&mut runtime, &project_store)
            .expect("已绑定卡片的第二次迁移应为空操作");
        let definitions = project_store
            .agent_definition_store()
            .list()
            .expect("迁移后应能列出 Agent 定义");
        let snapshot = project_store
            .canvas_store()
            .load_snapshot()
            .expect("迁移后应能读取 canvas snapshot")
            .expect("迁移后 canvas snapshot 应存在");

        assert_eq!(first, 2, "首次迁移应处理两张 definition_id 缺失的旧卡片");
        assert_eq!(second, 0, "卡片已绑定后再次迁移不得重复创建定义");
        assert_eq!(definitions.len(), 2, "两张旧卡片应恰好对应两条持久定义");
        assert!(
            definitions
                .iter()
                .any(|definition| definition.name == "小安"),
            "合法中文 alias 小安应原样保留为正式名称"
        );
        assert!(
            definitions
                .iter()
                .any(|definition| definition.name == "Alan"),
            "非法旧 alias 应回退到名称池首个可用名称 Alan"
        );
        assert!(
            !definitions
                .iter()
                .any(|definition| definition.name == "Code Reviewer"),
            "不符合新规则的 Code Reviewer 不得绕过名称校验进入定义"
        );
        assert!(
            snapshot.nodes.iter().all(|node| matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.definition_id.is_some()
            )),
            "持久化快照中的两张 Agent 卡片都必须写入 definition_id"
        );
        assert!(
            !snapshot.nodes.iter().any(|node| matches!(
                &node.kind,
                CanvasNodeKind::Agent(spec) if spec.alias.as_deref() == Some("Code Reviewer")
            )),
            "持久化快照不得残留非法旧 alias"
        );
    }
}

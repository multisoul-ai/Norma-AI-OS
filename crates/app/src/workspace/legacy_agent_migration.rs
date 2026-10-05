use agent::{AgentName, AgentNameRegistry};
use canvas::CanvasNodeKind;
use runtime::Runtime;
use storage::{AgentDefinition, NewAgentDefinition, ProjectStore, StorageError};

pub(crate) fn migrate_legacy_agent_cards(
    runtime: &mut Runtime,
    project_store: &ProjectStore,
) -> Result<usize, String> {
    purge_leaked_quick_definitions(runtime, project_store);
    let legacy = runtime
        .document
        .nodes()
        .iter()
        .filter_map(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            // Mobile cards intentionally have free-form titles and no roster definition.
            (spec.definition_id.is_none() && !spec.mobile_control)
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
        let node = runtime.document.node_mut(*node_id).ok_or_else(|| {
            ui::i18n::format(
                "workspace.legacy_agent_card_node_id_disappeared_during_migration",
                &[("node_id", (node_id).to_string())],
            )
        })?;
        let CanvasNodeKind::Agent(spec) = &mut node.kind else {
            return Err(ui::i18n::format(
                "workspace.legacy_agent_card_node_id_changed_kind_during_migration",
                &[("node_id", (node_id).to_string())],
            ));
        };
        spec.definition_id = Some(definition.id);
        spec.alias = Some(definition.name);
    }
    runtime
        .save_canvas_snapshot(&project_store.canvas_store())
        .map_err(|error| {
            ui::i18n::format(
                "workspace.could_not_save_migrated_agent_cards_error",
                &[("error", (error).to_string())],
            )
        })?;
    Ok(legacy.len())
}

/// 历史版本关闭 QuickCard 时清理可能因外键失败而遗留定义，占用内置名称。
fn purge_leaked_quick_definitions(runtime: &Runtime, project_store: &ProjectStore) {
    let live_ids = runtime
        .document
        .nodes()
        .iter()
        .filter_map(|node| match &node.kind {
            CanvasNodeKind::Agent(spec) => spec.definition_id,
            _ => None,
        })
        .collect::<Vec<_>>();
    match project_store
        .agent_definition_store()
        .purge_quick_except(&live_ids)
    {
        Ok(0) => {}
        Ok(purged) => tracing::info!(purged, "purged leaked quick Agent definitions"),
        Err(error) => tracing::warn!(%error, "could not purge leaked quick Agent definitions"),
    }
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
            Err(error) => {
                return Err(ui::i18n::format(
                    "workspace.could_not_migrate_legacy_agent_name_error",
                    &[("error", (error).to_string())],
                ))
            }
        }
    }

    let existing = store.list().map_err(|error| {
        ui::i18n::format(
            "workspace.could_not_list_agent_names_during_migration_error",
            &[("error", (error).to_string())],
        )
    })?;
    let registry = AgentNameRegistry::from_names(
        existing.iter().map(|definition| &definition.name),
    )
    .map_err(|error| {
        ui::i18n::format(
            "workspace.could_not_build_agent_name_registry_error",
            &[("error", (error).to_string())],
        )
    })?;
    let name = registry.next_available_name().ok_or_else(|| {
        ui::i18n::text("workspace.could_not_allocate_a_unique_name_for_a_legacy_agent_card")
            .to_string()
    })?;
    store
        .create(&NewAgentDefinition {
            name,
            runtime_profile: profile,
            workflow_description: String::new(),
        })
        .map_err(|error| {
            ui::i18n::format(
                "workspace.could_not_allocate_a_name_for_a_legacy_agent_card_error",
                &[("error", (error).to_string())],
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::{AgentProfile, AgentSpec, CanvasRect};

    /// 真正旧卡片在内置名称用尽时使用备用名称，重新加载迁移不得重复创建定义。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   已有定义 = 30 个内置名称 + AgentA = 31；下一个可用备用名称为 AgentB。
    ///   旧卡片 = 1 张 Codex，非法 alias="Code Reviewer"，非手机端且无 definition_id。
    ///   迁移后定义 = 31 + 1 = 32；再次从磁盘恢复后新增定义 = 0。
    ///
    /// 执行过程：
    ///   1. 填满名称池并创建旧卡片，执行真实迁移并保存快照。
    ///   2. 用新 Runtime 从磁盘加载，再次执行迁移。
    ///   3. 检查正式名称、稳定绑定与定义总数。
    ///
    /// 预期结果：
    ///   - 正断言：旧卡片绑定 AgentB，首次迁移 1 张，重开后绑定不变。
    ///   - 负断言：不得使用已占用的 AgentA，不得再次新增定义或保留非法名称。
    #[test]
    fn legacy_agent_cards_use_fallback_names_when_builtins_are_exhausted() {
        let temp = tempfile::tempdir().expect("应创建隔离项目");
        let store = ProjectStore::for_workspace(temp.path()).expect("应打开项目存储");
        for name in agent::builtin_agent_names()
            .iter()
            .map(|name| name.name())
            .chain(["AgentA"])
        {
            store
                .agent_definition_store()
                .create(&NewAgentDefinition {
                    name: name.to_string(),
                    runtime_profile: AgentProfile::Codex,
                    workflow_description: String::new(),
                })
                .expect("应写入已有定义");
        }
        let mut runtime = Runtime::new();
        // 640×400 沿用既有迁移测试尺寸，不影响名称分配。
        let node = runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: Some("Code Reviewer".into()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        );
        assert_eq!(
            migrate_legacy_agent_cards(&mut runtime, &store).expect("名称耗尽仍应迁移成功"),
            1,
            "首次必须迁移唯一旧卡片"
        );
        let definitions = store.agent_definition_store().list().expect("应列出定义");
        let migrated = definitions
            .iter()
            .find(|definition| definition.name == "AgentB")
            .expect("旧卡片应获得备用名称 AgentB");
        let mut reopened = Runtime::new();
        reopened
            .load_canvas_snapshot(&store.canvas_store())
            .expect("应恢复迁移后的快照");
        assert_eq!(
            migrate_legacy_agent_cards(&mut reopened, &store).expect("重开应成功"),
            0,
            "重开项目不得重复迁移"
        );
        let card = reopened.document.node(node).expect("旧卡片应仍存在");
        assert!(
            matches!(&card.kind, CanvasNodeKind::Agent(spec)
            if spec.definition_id == Some(migrated.id) && spec.alias.as_deref() == Some("AgentB")),
            "重开后的卡片必须保留 AgentB 及其稳定绑定"
        );
        assert!(
            !matches!(&card.kind, CanvasNodeKind::Agent(spec)
            if spec.alias.as_deref() == Some("Code Reviewer") || spec.alias.as_deref() == Some("AgentA")),
            "不得保留非法标题或复用被占用的 AgentA"
        );
        assert_eq!(
            store
                .agent_definition_store()
                .list()
                .expect("应列出定义")
                .len(),
            32,
            "两次迁移合计只能新增一条定义"
        );
    }

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

    /// 打开项目时清理泄漏的 QuickCard 定义，保留仍有卡片绑定的定义，内置名称随之归还。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   QuickCard Alan 有卡片（live），QuickCard Bella 无卡片（泄漏）；初始 2 条，清理 1 条后剩 1 条。
    ///
    /// 执行过程：
    ///   1. 创建两条 quick 定义，画布只放置绑定 Alan 的卡片。
    ///   2. 执行 migrate_legacy_agent_cards（内部先清理泄漏定义）。
    ///   3. 读取定义列表。
    ///
    /// 预期结果：
    ///   - 正断言：Alan 保留且卡片绑定不变。
    ///   - 负断言：Bella 被清理，不再占用内置名称。
    #[test]
    fn migration_purges_leaked_quick_definitions_but_keeps_bound_ones() {
        let temp = tempfile::tempdir().expect("应创建隔离项目");
        let store = ProjectStore::for_workspace(temp.path()).expect("应打开项目存储");
        let definitions = store.agent_definition_store();
        let new = |name: &str| NewAgentDefinition {
            name: name.to_string(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        };
        let alan = definitions.create_quick(&new("Alan")).expect("应创建 Alan");
        definitions
            .create_quick(&new("Bella"))
            .expect("应创建 Bella");
        let mut runtime = Runtime::new();
        runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: Some("Alan".into()),
                definition_id: Some(alan.id),
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        );

        let migrated = migrate_legacy_agent_cards(&mut runtime, &store).expect("迁移应成功");
        let remaining = definitions.list().expect("应列出定义");

        assert_eq!(migrated, 0, "已绑定卡片不属于旧卡片迁移");
        assert_eq!(remaining.len(), 1, "初始 2 条清理 1 条泄漏后应剩 1 条");
        assert_eq!(remaining[0].name, "Alan", "有卡片绑定的 Alan 必须保留");
        assert!(
            remaining.iter().all(|d| d.name != "Bella"),
            "泄漏的 Bella 必须被清理，归还内置名称"
        );
    }
}

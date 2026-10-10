use canvas::AgentProfile;
use storage::{AgentDefinitionStore, AgentDefinitionUpdate, NewAgentDefinition};

/// 名称规则接受首尾空白后，旧项目定义接口也必须保存规范显示名，避免主名与匹配键失配。
/// 数据构造：创建“  Alan  ”，随后改为换行/空白包围的 Bella；两者均有对应中文别名。
/// 执行过程：通过公开 create/update/list 重开读取，同一 definition id 不改变配置与身份。
/// 预期结果：主名分别保存 Alan 和 Bella，别名分别为艾伦和贝拉，数据库不保留首尾空白。
#[test]
fn definition_name_display_is_normalized_with_its_call_metadata() {
    let temp = tempfile::tempdir().expect("应建立隔离目录");
    let path = temp.path().join("project.sqlite");
    let store = AgentDefinitionStore::new(path.clone());
    let created = store
        .create(&NewAgentDefinition {
            name: "  Alan  ".into(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: "工作职责".into(),
        })
        .expect("首尾空白名称应按统一规则规范化");
    assert_eq!(created.name, "Alan", "旧定义创建也必须保存规范主名");
    assert_eq!(
        created.speech_alias.as_deref(),
        Some("艾伦"),
        "规范主名应派生对应语音别名"
    );
    let updated = store
        .update(&AgentDefinitionUpdate {
            id: created.id,
            name: "\n Bella \t".into(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: created.workflow_description,
            enabled: true,
            sort_order: 0,
        })
        .expect("改名应按同一规则规范化");
    let reopened = AgentDefinitionStore::new(path)
        .list()
        .expect("应重开读取")
        .into_iter()
        .find(|definition| definition.id == created.id)
        .expect("原定义仍存在");
    assert_eq!(updated.name, "Bella", "改名不能保存首尾空白");
    assert_eq!(reopened, updated, "规范主名和对应别名必须同时持久化");
    assert_eq!(
        reopened.speech_alias.as_deref(),
        Some("贝拉"),
        "旧别名不能继续作为新定义的显示别名"
    );
    assert_ne!(reopened.name, "\n Bella \t", "显示名不能与规范化匹配键失配");
}

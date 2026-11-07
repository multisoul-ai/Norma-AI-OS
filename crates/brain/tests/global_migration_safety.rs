use brain::migration::{active_dir, migrate};
use serde_json::{json, Value};
use std::path::Path;

fn write(path: impl AsRef<Path>, value: Value) {
    let path = path.as_ref();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

/// 权限偏好损坏不能在迁移后变成更宽松的默认值。
/// 数据：旧配置将project_writes写成字符串false，而不是JSON布尔值。
/// 执行：通过公开迁移入口读取旧配置。
/// 预期：返回InvalidData且不发布current，保留原文供修复。
#[test]
fn malformed_autonomy_is_rejected_instead_of_defaulting_to_permission() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    write(
        &path,
        json!({"enabled":true,"autonomy":{"project_writes":"false"}}),
    );
    let original = std::fs::read(&path).unwrap();
    let error = migrate(dir.path(), &[]).expect_err("损坏权限设置必须停止迁移");
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::InvalidData,
        "报告数据错误"
    );
    assert!(
        !active_dir(dir.path()).exists(),
        "不能发布会回退到允许写入的配置"
    );
    assert_eq!(std::fs::read(path).unwrap(), original, "原配置保留供修复");
}

/// 多个项目同时启动时只能发布一代，避免后完成的迁移覆盖新状态。
/// 数据：一个旧目标，四条线程用Barrier同时进入同一个全局目录。
/// 执行：各线程独立调用迁移，再读取唯一已发布目标。
/// 预期：恰好一次首次导入，其余复用；当前数据只有一条且没有重复代。
#[test]
fn concurrent_migrations_publish_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let source = brain::migration::WorkspaceSource {
        workspace_id: "alpha".into(),
        root: base.join("alpha"),
    };
    write(
        base.join("projects")
            .join(brain::migration::workspace_hash("alpha"))
            .join("goals.json"),
        json!([{"agent":"Alan","text":"one original goal"}]),
    );
    let start = std::sync::Barrier::new(4);
    let results = std::thread::scope(|threads| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                threads.spawn(|| {
                    start.wait();
                    migrate(base, std::slice::from_ref(&source)).unwrap()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results.iter().filter(|result| !result.reused).count(),
        1,
        "只有一个调用首次发布"
    );
    assert_eq!(
        results.iter().filter(|result| result.reused).count(),
        3,
        "其余调用复用已发布状态"
    );
    let goals = brain::goal::GoalStore::open(active_dir(base)).goals();
    assert_eq!(goals.len(), 1, "并发不得重复导入同一目标");
    assert_eq!(goals[0].text, "one original goal", "原目标完整");
    assert_eq!(
        std::fs::read_dir(base.join("global/generations"))
            .unwrap()
            .count(),
        1,
        "没有并发发布的多份数据"
    );
}

/// 新版本数据损坏后重启，不能从旧备份复活已经处理的工作。
/// 数据：先完成一次迁移，再破坏current的goals.json。
/// 执行：再次迁移同一旧目录。
/// 预期：错误保留给用户修复，入口和损坏文件不被自动替换。
#[test]
fn damaged_current_data_is_not_replaced_by_legacy_backups() {
    let dir = tempfile::tempdir().unwrap();
    let current = migrate(dir.path(), &[]).unwrap().directory;
    let generation = std::fs::read_link(&current).unwrap();
    std::fs::write(current.join("goals.json"), "broken current state").unwrap();
    let error = migrate(dir.path(), &[]).expect_err("当前损坏必须显式报错");
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::InvalidData,
        "不能把损坏当首次启动"
    );
    assert_eq!(
        std::fs::read_link(&current).unwrap(),
        generation,
        "不能切换到旧备份的新副本"
    );
    assert_eq!(
        std::fs::read_to_string(current.join("goals.json")).unwrap(),
        "broken current state",
        "损坏原文仍保留"
    );
}

/// 一份根目录提议不能同时变成两个画布可派发的提议。
/// 数据：alpha/beta元数据指向同一root，旧文件没有workspace身份。
/// 执行：迁移后分别检查两个项目路径与归档路径。
/// 预期：只保留一份有明确归档身份的数据，不猜测归属、不重复派发。
#[test]
fn ambiguous_root_proposals_are_archived_once() {
    use brain::migration::{proposal_file, workspace_hash, WorkspaceSource};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared-project");
    let hash = workspace_hash(&root.to_string_lossy());
    let old = dir
        .path()
        .join("proposals")
        .join(format!("{}.json", &hash[..16]));
    write(
        old,
        json!({"next_id":1,"items":[{"id":1,"agent":"Alan","title":"old","task":"preserve full task","why":"old reason","at":10}]}),
    );
    let sources = ["alpha", "beta"].map(|id| WorkspaceSource {
        workspace_id: id.into(),
        root: root.clone(),
    });
    let result = migrate(dir.path(), &sources).unwrap();
    let archived = format!("archived:root:{}", &hash[..16]);
    assert_eq!(
        result.archived_workspaces,
        vec![archived.clone()],
        "归档只登记一次"
    );
    for id in ["alpha", "beta"] {
        assert!(
            !proposal_file(&result.directory, id).exists(),
            "不得把不明提议指派给{id}"
        );
    }
    let saved: Value = serde_json::from_slice(
        &std::fs::read(proposal_file(&result.directory, &archived)).unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved["items"].as_array().unwrap().len(),
        1,
        "原提议恰保留一次"
    );
    assert_eq!(
        saved["items"][0]["task"], "preserve full task",
        "完整任务保留"
    );
}

/// current缺失整份提议目录时，不能把丢失的数据解释成用户已处理全部提议。
/// 数据：完成迁移后将提议目录移出current，模拟不完整恢复。
/// 执行：重跑迁移并检查错误、原入口和移出的目录。
/// 预期：拒绝复用，不生成空集合来掩盖丢失，也不重新导入旧备份。
#[test]
fn missing_current_proposals_directory_prevents_reuse() {
    let dir = tempfile::tempdir().unwrap();
    let current = migrate(dir.path(), &[]).unwrap().directory;
    let target = std::fs::read_link(&current).unwrap();
    let moved = dir.path().join("displaced-proposals");
    std::fs::rename(current.join("proposals"), &moved).unwrap();
    assert!(
        migrate(dir.path(), &[]).is_err(),
        "缺失已发布数据目录必须报错"
    );
    assert_eq!(
        std::fs::read_link(&current).unwrap(),
        target,
        "不切换全局入口"
    );
    assert!(
        !current.join("proposals").exists(),
        "不以空目录覆盖丢失状态"
    );
    assert!(moved.exists(), "可供人工恢复的目录仍保留");
}

/// 各项目的权限冲突不能按目录顺序扩大为App全局授权。
/// 数据：alpha关闭幕僚/项目写入且未开启派活；beta全开；分别有/无显式App授权。
/// 执行：迁移后读取唯一配置。
/// 预期：无App显式覆盖时取项目权限交集，缺字段按原默认；有显式App授权时保留它。
#[test]
fn project_permissions_merge_conservatively_unless_app_explicitly_overrides() {
    use brain::migration::{workspace_hash, WorkspaceSource};
    for explicit_app_permission in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let sources = ["alpha", "beta"].map(|id| WorkspaceSource {
            workspace_id: id.into(),
            root: base.join(id),
        });
        write(
            base.join("projects")
                .join(workspace_hash("alpha"))
                .join("config.json"),
            json!({"enabled":false,"autonomy":{"project_writes":false}}),
        );
        write(
            base.join("projects")
                .join(workspace_hash("beta"))
                .join("config.json"),
            json!({"enabled":true,"autonomy":{"project_writes":true,"assign_followups":true}}),
        );
        if explicit_app_permission {
            write(
                base.join("config.json"),
                json!({"enabled":true,"autonomy":{"project_writes":true,"assign_followups":true}}),
            );
        }
        let result = migrate(base, &sources).unwrap();
        let saved: Value =
            serde_json::from_slice(&std::fs::read(result.directory.join("config.json")).unwrap())
                .unwrap();
        assert_eq!(
            saved["enabled"], explicit_app_permission,
            "没有App授权时不能自动启用其他项目"
        );
        assert_eq!(
            saved["autonomy"]["project_writes"], explicit_app_permission,
            "不能以beta的写权限覆盖alpha的拒绝"
        );
        assert_eq!(
            saved["autonomy"]["assign_followups"], explicit_app_permission,
            "alpha缺失派活字段按原默认false合并"
        );
        assert_eq!(
            saved["autonomy"]["readonly_commands"], true,
            "双方原默认只读权限保持"
        );
    }
}

/// 损坏的提议身份不能被目录推断覆盖，也不能留下两个可操作的相同编号。
/// 数据：分别构造错误类型的workspace_id及一个文件内重复的提议编号。
/// 执行：迁移两个独立旧目录，核对错误和原文件。
/// 预期：两种情况都拒绝发布，不能静默重绑身份或让用户处理错误提议。
#[test]
fn corrupt_proposal_identity_or_duplicate_ids_abort_migration() {
    use brain::migration::{workspace_hash, WorkspaceSource};
    for duplicate_id in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let item =
            json!({"id":1,"agent":"Alan","title":"old","task":"full task","why":"reason","at":10});
        let mut saved = json!({"next_id":1,"items":[item.clone()]});
        if duplicate_id {
            let mut second = item;
            second["task"] = json!("another complete task");
            saved["items"].as_array_mut().unwrap().push(second);
        } else {
            saved["workspace_id"] = json!(123);
        }
        let path = base
            .join("projects")
            .join(workspace_hash("alpha"))
            .join("proposals.json");
        write(&path, saved);
        let original = std::fs::read(&path).unwrap();
        let source = WorkspaceSource {
            workspace_id: "alpha".into(),
            root: base.join("alpha"),
        };
        let error = migrate(base, &[source]).expect_err("损坏提议必须拒绝发布");
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::InvalidData,
            "身份错误应显式暴露"
        );
        assert!(!active_dir(base).exists(), "不发布会错绑身份的提议");
        assert_eq!(std::fs::read(path).unwrap(), original, "原文件留存供修复");
    }
}

use brain::migration::{active_dir, migrate, proposal_file, workspace_hash, WorkspaceSource};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn write(path: impl AsRef<Path>, value: Value) {
    let path = path.as_ref();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn source(name: &str, base: &Path) -> WorkspaceSource {
    WorkspaceSource {
        workspace_id: name.into(),
        root: base.join(name),
    }
}

fn project(base: &Path, id: &str) -> PathBuf {
    base.join("projects").join(workspace_hash(id))
}

fn proposals(task: &str) -> Value {
    json!({"next_id": 1, "items": [{"id":1,"agent":"Alan","title":"检查边界","task":task,"why":"先补验证","at":100}]})
}

/// 两个旧工作空间完整迁移，目标/提议/日志/偏好均带来源，原目录字节不变。
/// 数据：alpha/beta同名Agent、同提议ID；根配置和项目配置同时存在；已有global目标也必须保留。
/// 执行：迁移后通过GoalStore及JSON读取所有数据，核对备份、偏好范围与设置优先级。
/// 预期：不同空间不合并目标/提议，根配置保持权威，全局与项目的always_ask作用域不被缩小或扩大。
#[test]
fn migration_keeps_all_scoped_data_and_leaves_originals_intact() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let sources = vec![source("alpha", base), source("beta", base)];
    write(
        base.join("config.json"),
        json!({"enabled":true,"backend":"codex","speak":false,"always_ask":["Write"]}),
    );
    let mut backups = Vec::new();
    for id in ["alpha", "beta"] {
        let old = project(base, id);
        write(
            old.join("goals.json"),
            json!([{"agent":"Alan","text":format!("{id} original goal")} ]),
        );
        write(
            old.join("proposals.json"),
            proposals(&format!("{id} full task")),
        );
        write(
            old.join("config.json"),
            json!({"backend":"claude","always_ask":["Bash:cargo"]}),
        );
        std::fs::write(
            old.join("decisions.jsonl"),
            format!("{}\n", json!({"lane":"Main","reply":format!("{id} reply")})),
        )
        .unwrap();
        for name in [
            "goals.json",
            "proposals.json",
            "config.json",
            "decisions.jsonl",
        ] {
            let path = old.join(name);
            backups.push((path.clone(), std::fs::read(path).unwrap()));
        }
    }
    write(
        base.join("global/goals.json"),
        json!([{"workspace_id":"gamma","agent":"Grace","text":"new global goal"}]),
    );
    let result = migrate(base, &sources).expect("应完成原子迁移");
    assert!(!result.reused, "首次必须实际导入");
    assert_eq!(result.directory, active_dir(base), "返回稳定全局入口");
    let goals = brain::goal::GoalStore::open(result.directory.clone()).goals();
    assert_eq!(goals.len(), 3, "两个旧目标和已有全局目标都保留");
    for id in ["alpha", "beta", "gamma"] {
        assert_eq!(
            goals.iter().filter(|g| g.workspace_id == id).count(),
            1,
            "{id}恰有原目标"
        );
    }
    for id in ["alpha", "beta"] {
        let saved = read(proposal_file(&result.directory, id));
        assert_eq!(saved["items"][0]["workspace_id"], id, "提议有准确来源");
        assert_eq!(saved["items"][0]["id"], 1, "跨空间同ID无需改写");
        assert_eq!(
            saved["items"][0]["task"],
            format!("{id} full task"),
            "完整任务不截断"
        );
    }
    let decisions: Vec<Value> = std::fs::read_to_string(result.directory.join("decisions.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(decisions.len(), 2, "两条决策日志均迁移");
    assert!(
        decisions
            .iter()
            .any(|item| item["workspace_id"] == "alpha" && item["reply"] == "alpha reply"),
        "alpha日志正确归属"
    );
    assert!(
        decisions
            .iter()
            .any(|item| item["workspace_id"] == "beta" && item["reply"] == "beta reply"),
        "beta日志正确归属"
    );
    let settings = read(result.directory.join("config.json"));
    assert_eq!(
        settings["backend"], "codex",
        "已有App配置不能被项目配置覆盖"
    );
    assert_eq!(settings["speak"], false, "用户静音偏好保持");
    let rules: Vec<brain::preferences::AskRule> =
        serde_json::from_value(settings["always_ask"].clone()).unwrap();
    assert_eq!(rules.len(), 3, "一个App级偏好和两个项目偏好都保留");
    assert!(
        rules
            .iter()
            .any(|r| r.matches(Some("future-project"), "Write")),
        "旧全局规则仍保护将来项目"
    );
    assert!(
        !rules.iter().any(|r| r.matches(Some("gamma"), "Bash:cargo")),
        "项目规则不能扩大至gamma"
    );
    for id in ["alpha", "beta"] {
        assert!(
            rules.iter().any(|r| r.matches(Some(id), "Bash:cargo")),
            "{id}原确认规则保留"
        );
    }
    for (path, original) in backups {
        assert_eq!(
            std::fs::read(path).unwrap(),
            original,
            "旧目录作为原样备份保留"
        );
    }
}

/// 重跑迁移不能复活已处理提议，也不能覆盖新目标或设置。
/// 数据：迁移alpha后模拟新版本处理提议、记录新目标、关闭幕僚。
/// 执行：再次迁移相同旧目录，比较文件字节和原子入口目标。
/// 预期：复用同一代，所有新状态保持，旧提议不会再次导入。
#[test]
fn migration_is_idempotent_and_never_overwrites_new_global_changes() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let sources = vec![source("alpha", base)];
    write(
        project(base, "alpha").join("proposals.json"),
        proposals("old task"),
    );
    migrate(base, &sources).unwrap();
    let current = active_dir(base);
    let generation = std::fs::read_link(&current).unwrap();
    write(
        proposal_file(&current, "alpha"),
        json!({"workspace_id":"alpha","next_id":2,"items":[]}),
    );
    write(
        current.join("goals.json"),
        json!([{"workspace_id":"alpha","agent":"Alan","text":"new goal"}]),
    );
    write(
        current.join("config.json"),
        json!({"enabled":false,"always_ask":[]}),
    );
    let before = [
        current.join("goals.json"),
        current.join("config.json"),
        proposal_file(&current, "alpha"),
    ]
    .map(|path| (path.clone(), std::fs::read(path).unwrap()));
    let result = migrate(base, &sources).unwrap();
    assert!(result.reused, "第二次不得重新导入");
    assert_eq!(
        std::fs::read_link(current).unwrap(),
        generation,
        "同一代入口保持不变"
    );
    for (path, bytes) in before {
        assert_eq!(
            std::fs::read(path).unwrap(),
            bytes,
            "新状态不得被旧备份覆盖"
        );
    }
}

/// 损坏任一旧数据文件都必须中止发布，不能当成空集合或部分完成。
/// 数据：alpha目标有效，beta提议缺失必填task；原文件作为修复依据保留。
/// 执行：首次迁移失败，检查没有current；修复原文件后重试。
/// 预期：失败不发布任何半套状态，不改备份；修复后可完整迁移。
#[test]
fn corrupt_legacy_file_aborts_publication_and_can_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let sources = vec![source("alpha", base), source("beta", base)];
    write(
        project(base, "alpha").join("goals.json"),
        json!([{"agent":"Alan","text":"original target"}]),
    );
    let broken = project(base, "beta").join("proposals.json");
    write(
        &broken,
        json!({"next_id":1,"items":[{"id":1,"agent":"Alan","title":"kept","why":"kept","at":10}]}),
    );
    let original = std::fs::read(&broken).unwrap();
    let error = migrate(base, &sources).expect_err("损坏提议必须失败");
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::InvalidData,
        "格式错误可区分于IO故障"
    );
    assert!(!active_dir(base).exists(), "任何错误都不能发布半套全局数据");
    assert_eq!(
        std::fs::read(&broken).unwrap(),
        original,
        "损坏原文保留供修复"
    );
    write(&broken, proposals("repaired full task"));
    let result = migrate(base, &sources).unwrap();
    assert_eq!(
        read(result.directory.join("goals.json"))
            .as_array()
            .unwrap()
            .len(),
        1,
        "修复后原目标仍在"
    );
    assert_eq!(
        read(proposal_file(&result.directory, "beta"))["items"][0]["task"],
        "repaired full task",
        "重试导入修复提议"
    );
}

/// 已删除项目只剩哈希目录时，应保留为独立归档身份，不能猜成当前打开项目。
/// 数据：未知项目目录与已知alpha同时存在，但只有alpha的元数据可用。
/// 执行：迁移并读取目标与归档清单。
/// 预期：未知记录完整保留、使用可追溯归档身份，不落入alpha。
#[test]
fn unmapped_project_is_archived_without_reassigning_it_to_an_open_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let hash = workspace_hash("deleted-workspace");
    write(
        base.join("projects").join(&hash).join("goals.json"),
        json!([{"agent":"Alan","text":"archived task"}]),
    );
    let result = migrate(base, &[source("alpha", base)]).unwrap();
    let archived = format!("archived:{hash}");
    assert_eq!(
        result.archived_workspaces,
        vec![archived.clone()],
        "显式保留身份缺失的来源"
    );
    let goals = brain::goal::GoalStore::open(result.directory).goals();
    assert_eq!(
        goals[0].workspace_id, archived,
        "归档身份来自原目录而非猜测"
    );
    assert_ne!(goals[0].workspace_id, "alpha", "不得错误关联当前项目");
}

/// 更旧版本按项目根路径保存提议；同一工作空间另有同编号的新提议。
/// 数据：旧路径文件和alpha目录各有id=1，但任务不同，global另有第三条同编号提议。
/// 执行：迁移三处文件，再读全局提议集合并核对旧文件字节。
/// 预期：完整保留三条不同任务，重编号只解决同空间冲突，旧文件不移动也不删除。
#[test]
fn root_based_proposals_merge_without_losing_colliding_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let alpha = source("alpha", base);
    let hash = workspace_hash(&alpha.root.to_string_lossy());
    let legacy = base.join("proposals").join(format!("{}.json", &hash[..16]));
    write(&legacy, proposals("root based complete task"));
    let original = std::fs::read(&legacy).unwrap();
    write(
        project(base, "alpha").join("proposals.json"),
        proposals("project complete task"),
    );
    let mut global = proposals("global complete task");
    global["workspace_id"] = json!("alpha");
    write(proposal_file(&base.join("global"), "alpha"), global);
    let result = migrate(base, &[alpha]).unwrap();
    let saved = read(proposal_file(&result.directory, "alpha"));
    let items = saved["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "三个不同的完整任务不能因为编号相同而丢失");
    for task in [
        "root based complete task",
        "project complete task",
        "global complete task",
    ] {
        assert_eq!(
            items.iter().filter(|item| item["task"] == task).count(),
            1,
            "每条原任务恰保留一次"
        );
    }
    let ids: std::collections::BTreeSet<_> = items
        .iter()
        .map(|item| item["id"].as_u64().unwrap())
        .collect();
    assert_eq!(ids.len(), 3, "合并后可按唯一编号处理每条提议");
    assert_eq!(
        std::fs::read(legacy).unwrap(),
        original,
        "更旧提议文件也原样保留"
    );
}

/// App旧审批决策日志与后台LLM决策日志都是用户历史，必须一起迁移。
/// 数据：alpha只有approvals.jsonl，beta只有decisions.jsonl。
/// 执行：迁移两个项目并读取统一全局日志。
/// 预期：两条原记录内容和来源均保留，旧审批日志字节不变。
#[test]
fn legacy_approval_audit_is_preserved_in_the_global_decision_log() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let alpha = project(base, "alpha");
    let beta = project(base, "beta");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::create_dir_all(&beta).unwrap();
    let original =
        "{\"approval\":\"request-a\",\"decision\":\"Deny\",\"reason\":\"keep original reason\"}\n";
    std::fs::write(alpha.join("approvals.jsonl"), original).unwrap();
    std::fs::write(
        beta.join("decisions.jsonl"),
        "{\"reply\":\"beta original reply\"}\n",
    )
    .unwrap();
    let result = migrate(base, &[source("alpha", base), source("beta", base)]).unwrap();
    let decisions: Vec<Value> = std::fs::read_to_string(result.directory.join("decisions.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(decisions.len(), 2, "App审批与LLM日志都保留");
    assert!(
        decisions.iter().any(|item| item["workspace_id"] == "alpha"
            && item["approval"] == "request-a"
            && item["reason"] == "keep original reason"),
        "审批日志完整且属于alpha"
    );
    assert!(
        decisions
            .iter()
            .any(|item| item["workspace_id"] == "beta" && item["reply"] == "beta original reply"),
        "后台日志仍属于beta"
    );
    assert_eq!(
        std::fs::read_to_string(alpha.join("approvals.jsonl")).unwrap(),
        original,
        "原审批日志字节保持"
    );
}

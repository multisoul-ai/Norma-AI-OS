use storage::connector_store::{ConnectorStore, TokenRecord};

/// 场景：新 App 无连接器配置；构造临时库，读取配置并保存令牌和审计。
/// 预期：默认关闭、端口 47615，持久化包含明文，审计保留结果码。
#[test]
fn defaults_and_plaintext_persistence() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let path = dir.path().join("app.sqlite");
    let store = ConnectorStore::new(path.clone());
    let config = store.config().expect("默认配置");
    assert!(!config.enabled, "新连接器必须关闭");
    assert_eq!(config.port, 47615, "规格默认端口");
    let token = TokenRecord {
        token: random_token(),
        created_at: 100,
        expires_at: None,
    };
    store
        .replace_tokens(std::slice::from_ref(&token))
        .expect("保存令牌");
    assert_eq!(
        store.tokens().expect("读取令牌"),
        vec![token],
        "令牌必须持久保存"
    );
    store
        .audit(100, "authentication", "unauthorized")
        .expect("写审计");
    assert_eq!(
        store.audit_page(20).expect("审计")[0].result_code,
        "unauthorized",
        "鉴权失败必须可审计"
    );
    store.replace_tokens(&[]).expect("吊销");
    assert!(store.tokens().expect("读取").is_empty(), "吊销移除所有令牌");
}

/// 场景：同一空间将同目录添加两次，各自保存node_id=1，另加未初始化区域。
/// 执行：读取connector目录，再重开读取；预期画布标识独立稳定，读取不创建缺失画布。
#[test]
fn catalog_uses_stable_per_area_canvas_and_does_not_initialize_missing_files() {
    use canvas::{
        AgentProfile, AgentSpec, CanvasNodeKind, CanvasNodeSnapshot, CanvasRect, DocumentSnapshot,
        Viewport,
    };
    use storage::{DatabasePaths, DesktopWorkspaceStore, NewAgentDefinition, ProjectStore};
    let dir = tempfile::tempdir().expect("隔离目录");
    let project = dir.path().join("project");
    std::fs::create_dir(&project).expect("目录");
    let paths = DatabasePaths::isolated_test(&dir.path().join("data"));
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let space = catalog.create("空间").expect("空间");
    let a = catalog.add_project(&space.id, &project).expect("画布A");
    let b = catalog.add_project(&space.id, &project).expect("画布B");
    let missing = catalog
        .add_project(&space.id, &project)
        .expect("未加载区域");
    let definitions =
        ProjectStore::initialize_for_desktop_definitions(&paths, &a.project_key).expect("定义库");
    let definition = definitions
        .agent_definition_store()
        .create(&NewAgentDefinition {
            name: "Alan".into(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("定义");
    let snapshot = DocumentSnapshot {
        nodes: vec![CanvasNodeSnapshot {
            id: 1,
            kind: CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: None,
                definition_id: Some(definition.id),
                mobile_control: false,
            }),
            bounds: CanvasRect::new(0.0, 0.0, 300.0, 200.0),
            z_index: 0,
        }],
        viewport: Viewport::default(),
    };
    for area in [&a, &b] {
        ProjectStore::initialize_for_desktop_area(&paths, &project, &space.id, &area.id)
            .expect("画布")
            .canvas_store()
            .save_snapshot(&snapshot)
            .expect("保存");
    }
    let reader = storage::connector_read::ConnectorReadStore::new(paths.app_db_path().to_owned());
    let first = reader.read().expect("读取目录");
    let areas = &first[0].areas;
    let canvas_a = areas.iter().find(|e| e.area.id == a.id).expect("A");
    let canvas_b = areas.iter().find(|e| e.area.id == b.id).expect("B");
    assert_ne!(
        canvas_a.canvas_id, canvas_b.canvas_id,
        "同目录必须有独立持久画布标识"
    );
    assert_eq!(
        canvas_a.members[0].node_id, canvas_b.members[0].node_id,
        "构造相同node_id"
    );
    assert_eq!(
        canvas_a.members[0].name, canvas_b.members[0].name,
        "构造同名卡片"
    );
    let reopened = reader.read().expect("重开");
    assert_eq!(
        areas.iter().map(|a| &a.canvas_id).collect::<Vec<_>>(),
        reopened[0]
            .areas
            .iter()
            .map(|a| &a.canvas_id)
            .collect::<Vec<_>>(),
        "读取重开身份稳定"
    );
    let entry = areas
        .iter()
        .find(|e| e.area.id == missing.id)
        .expect("未加载区域");
    assert!(entry.members.is_empty(), "缺失快照不伪造卡片");
    assert!(
        !paths
            .app_db_path()
            .parent()
            .expect("父目录")
            .join("desktop-canvases")
            .join(format!("{}.sqlite", entry.canvas_id))
            .exists(),
        "只读查询不能初始化缺失画布"
    );
    let config = storage::connector_store::ConnectorConfig {
        enabled: true,
        port: 47616,
        tunnel_authority: None,
    };
    let store = ConnectorStore::new(paths.app_db_path().to_owned());
    store.save_config(&config).expect("设置端口");
    assert_eq!(
        ConnectorStore::new(paths.app_db_path().to_owned())
            .config()
            .expect("重开配置"),
        config,
        "端口设置持久化"
    );
}

/// 场景：用随机数据构造令牌保存明文，并在同库写审计；执行原始文件检索。
/// 预期：SQLite文件包含明文，审计不包含明文；90天前审计被清理。
#[test]
fn sqlite_has_plaintext_but_audit_expires_after_ninety_days() {
    use std::io::Read;
    let dir = tempfile::tempdir().expect("目录");
    let path = dir.path().join("app.sqlite");
    let mut entropy = [0; 32];
    std::fs::File::open("/dev/urandom")
        .expect("随机源")
        .read_exact(&mut entropy)
        .expect("随机数据");
    let token = format!(
        "nrm_live_{}",
        entropy
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let store = ConnectorStore::new(path.clone());
    store
        .replace_tokens(&[TokenRecord {
            token: token.clone(),
            created_at: 100,
            expires_at: None,
        }])
        .expect("保存摘要");
    store
        .audit(1, "authentication", "unauthorized")
        .expect("旧审计");
    // 90×24×60×60 = 7,776,000；91天后的插入应清理第一条。
    store.audit(91 * 86400, "directory", "ok").expect("新审计");
    assert_eq!(store.audit_page(50).expect("审计").len(), 1, "超过90天清理");
    let bytes = std::fs::read(path).expect("读取库文件");
    assert!(
        bytes.windows(token.len()).any(|w| w == token.as_bytes()),
        "数据库保存明文令牌"
    );
}

/// 场景：旧画布存在未绑定definition的Agent卡片，定义库尚未创建。
/// 执行：只读connector目录；预期卡片仍可见，不能因定义缺失把有卡片的画布报为空。
#[test]
fn legacy_unbound_agent_remains_visible_without_definition_database() {
    use canvas::{
        AgentProfile, AgentSpec, CanvasNodeKind, CanvasNodeSnapshot, CanvasRect, DocumentSnapshot,
        Viewport,
    };
    use storage::{DatabasePaths, DesktopWorkspaceStore, ProjectStore};
    let dir = tempfile::tempdir().expect("目录");
    let project = dir.path().join("project");
    std::fs::create_dir(&project).expect("项目目录");
    let paths = DatabasePaths::isolated_test(&dir.path().join("data"));
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("空间");
    let area = catalog.add_project(&workspace.id, &project).expect("区域");
    ProjectStore::initialize_for_desktop_area(&paths, &project, &workspace.id, &area.id)
        .expect("画布")
        .canvas_store()
        .save_snapshot(&DocumentSnapshot {
            nodes: vec![CanvasNodeSnapshot {
                id: 1,
                kind: CanvasNodeKind::Agent(AgentSpec {
                    profile: AgentProfile::Codex,
                    alias: Some("Legacy".into()),
                    definition_id: None,
                    mobile_control: false,
                }),
                bounds: CanvasRect::new(0.0, 0.0, 300.0, 200.0),
                z_index: 0,
            }],
            viewport: Viewport::default(),
        })
        .expect("保存旧画布");
    let result = storage::connector_read::ConnectorReadStore::new(paths.app_db_path().to_owned())
        .read()
        .expect("只读查询");
    let cards = &result[0].areas[0].members;
    assert_eq!(cards.len(), 1, "未绑定定义的Agent卡片不能丢失");
    assert_eq!(cards[0].name, "Legacy", "保留旧卡片可读名称");
}

/// 场景：旧配置没有隧道字段；新配置允许hostname[:port]并通过重开持久化。
/// 构造：直接反序列化旧JSON和恶意URL/通配符/越界端口，再保存合法ts.net主机。
/// 预期：旧配置为空，合法配置往返一致；所有非authority输入在保存前拒绝。
#[test]
fn tunnel_authority_validates_and_round_trips_without_breaking_old_config() {
    use storage::connector_store::ConnectorConfig;
    let old: ConnectorConfig =
        serde_json::from_str(r#"{"enabled":true,"port":47615}"#).expect("旧配置");
    assert!(old.tunnel_authority.is_none(), "旧配置默认无隧道");
    for value in [
        "https://host.ts.net",
        "host.ts.net/mcp",
        "*.ts.net",
        "user@host.ts.net",
        "host:0",
        "host:65536",
        "host:",
        "bad_host",
        "-host",
        "host..ts.net",
        "host\r\n",
        "",
    ] {
        assert!(
            serde_json::from_value::<ConnectorConfig>(
                serde_json::json!({"tunnel_authority":value})
            )
            .is_err(),
            "拒绝非法authority: {value:?}"
        );
    }
    let dir = tempfile::tempdir().expect("目录");
    let store = ConnectorStore::new(dir.path().join("app.sqlite"));
    for value in [
        "alanmacbook-pro.tailnet.ts.net",
        "alanmacbook-pro.tailnet.ts.net:8443",
    ] {
        let config: ConnectorConfig =
            serde_json::from_value(serde_json::json!({"tunnel_authority":value}))
                .expect("合法主机");
        store.save_config(&config).expect("保存");
        assert_eq!(store.config().expect("重开"), config, "主机和端口精确保留");
    }
}

/// 场景：同一IP窗口内110次失败，然后另一个IP和同IP新窗口失败。
/// 执行：调用专用失败审计接口并通过audit_page读取；预期同窗口1行计数110，其他窗口独立。
#[test]
fn authentication_flood_updates_one_audit_row_per_source_window() {
    let dir = tempfile::tempdir().expect("目录");
    let store = ConnectorStore::new(dir.path().join("app.sqlite"));
    for count in 1..=110 {
        store
            .audit_auth_failure(
                100,
                "198.51.100.1",
                100,
                count,
                if count >= 10 { 1000 } else { 0 },
            )
            .expect("审计");
    }
    let page = store.audit_page(50).expect("审计页");
    assert_eq!(page.len(), 1, "锁定洪水只保留一行");
    assert_eq!(page[0].failure_count, 110, "合并计数不丢失");
    assert_eq!(page[0].result_code, "unauthorized", "不存令牌和请求参数");
    store
        .audit_auth_failure(100, "198.51.100.2", 100, 1, 0)
        .expect("其他IP");
    store
        .audit_auth_failure(1000, "198.51.100.1", 1000, 1, 0)
        .expect("新窗口");
    assert_eq!(
        store.audit_page(50).expect("审计页").len(),
        3,
        "各IP和窗口独立"
    );
}

/// 场景：随机令牌及两种独立密钥存入临时 SQLite，销毁 store 后重新打开两次。
/// 构造：随机 UUID 文本、创建时间100、旧令牌宽限至86500（100+24×60×60）。
/// 执行：保存当前/宽限令牌和密钥，重开读取，再吊销；预期明文可查看，Debug脱敏，吊销删除行。
#[test]
fn plaintext_credentials_survive_reopen_and_revoke() {
    use storage::connector_store::TokenRecord;
    let dir = tempfile::tempdir().expect("隔离目录");
    let path = dir.path().join("app.sqlite");
    let store = ConnectorStore::new(path.clone());
    let current = random_token();
    let old = random_token();
    let tokens = vec![
        TokenRecord {
            token: current.clone(),
            created_at: 100,
            expires_at: None,
        },
        TokenRecord {
            token: old.clone(),
            created_at: 50,
            expires_at: Some(86500),
        },
    ];
    store.replace_tokens(&tokens).expect("保存明文");
    let hmac = random_token().into_bytes();
    let card = random_token().into_bytes();
    store
        .secret_or_insert("webhook-hmac", &hmac, 100)
        .expect("HMAC");
    store
        .secret_or_insert("card-identity-hmac", &card, 100)
        .expect("card_id");
    drop(store);
    for _ in 0..2 {
        let reopened = ConnectorStore::new(path.clone());
        assert!(
            reopened
                .tokens()
                .expect("重开明文")
                .iter()
                .all(|t| tokens.contains(t))
                && reopened.tokens().expect("计数").len() == 2,
            "明文与生命周期元数据保持"
        );
        assert!(
            reopened
                .secret_or_insert("webhook-hmac", &card, 200)
                .expect("HMAC重开")
                == hmac,
            "既有HMAC不能被覆盖"
        );
        assert!(
            reopened
                .secret_or_insert("card-identity-hmac", &hmac, 200)
                .expect("card_id重开")
                == card,
            "card_id密钥独立稳定"
        );
        let debug = format!("{:?}", reopened.tokens().expect("Debug"));
        assert!(debug.contains("REDACTED"), "必须明确脱敏");
        assert!(
            !debug.contains(&current) && !debug.contains(&old),
            "Debug不得泄漏令牌"
        );
    }
    ConnectorStore::new(path.clone())
        .replace_tokens(&[])
        .expect("吊销");
    assert!(
        ConnectorStore::new(path).tokens().expect("重开").is_empty(),
        "吊销后无明文记录"
    );
}

fn random_token() -> String {
    use std::io::Read;
    let mut bytes = [0; 32];
    std::fs::File::open("/dev/urandom")
        .expect("随机源")
        .read_exact(&mut bytes)
        .expect("随机数据");
    format!(
        "nrm_live_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

/// 场景：未发布旧构建只有摘要表；构造临时旧表与审计，连续执行迁移并重开。
/// 执行：插入旧摘要与审计，migrate_app_db两次；预期旧摘要清理，新明文表可用，审计保留。
#[test]
fn legacy_hash_schema_migration_is_idempotent_and_preserves_audit() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let path = dir.path().join("app.sqlite");
    let conn = rusqlite::Connection::open(&path).expect("旧临时库");
    conn.execute_batch("CREATE TABLE connector_token_hashes(token_hash BLOB PRIMARY KEY NOT NULL,expires_at_unix INTEGER);
        INSERT INTO connector_token_hashes VALUES(zeroblob(32),NULL);
        CREATE TABLE connector_audit(sequence INTEGER PRIMARY KEY AUTOINCREMENT,occurred_at_unix INTEGER NOT NULL,tool TEXT NOT NULL,parameter_summary TEXT NOT NULL,operation_id TEXT,result_code TEXT NOT NULL,user_quote TEXT);
        INSERT INTO connector_audit(occurred_at_unix,tool,parameter_summary,result_code) VALUES(100,'directory','','ok');").expect("旧schema");
    for _ in 0..2 {
        storage::migrations::migrate_app_db(&conn).expect("幂等迁移");
    }
    let old_tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='connector_token_hashes'",
            [],
            |r| r.get(0),
        )
        .expect("旧表");
    assert_eq!(
        old_tables, 0,
        "不可恢复的旧摘要表已清理，不访问旧秘密提供者"
    );
    drop(conn);
    let reopened = ConnectorStore::new(path);
    assert!(
        reopened.tokens().expect("新凭据表").is_empty(),
        "迁移不伪造旧令牌"
    );
    assert_eq!(
        reopened.audit_page(20).expect("旧审计")[0].result_code,
        "ok",
        "迁移保留既有审计"
    );
    reopened
        .replace_tokens(&[storage::connector_store::TokenRecord {
            token: random_token(),
            created_at: 200,
            expires_at: None,
        }])
        .expect("新表可写");
    assert_eq!(
        reopened.tokens().expect("重开").len(),
        1,
        "迁移后明文可持久化"
    );
}

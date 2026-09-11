use app::{LiveSnapshotRequest, MobileAccess, MobileServer};
use norma::mobile::{CardSnapshot, ServerIdentity};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::Command,
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};
#[path = "support/mobile_card_creation_ui.rs"]
mod card_ui;
#[path = "support/mobile_decision_ui.rs"]
mod decision_ui;
#[path = "support/mobile_delivery_ui.rs"]
mod delivery_ui;
#[path = "support/mobile_direction_ui.rs"]
mod direction_ui;
#[path = "support/mobile_todo_ui.rs"]
mod todo_ui;

/// 原生通知状态使用真实TLS配对/Keychain/SQLite；UI模式保留同一服务供Computer Use回源。
///
/// 数据构造：仅回环监听、随机TLS身份、单只读项目、合成完成卡；UI模式另加一张空白Claude卡，不运行Provider。
/// 执行过程：签发隔离QR → 原生测试发起配对 → 仅批准测试手机 → 验证订阅生命周期 → 撤销并回收。
/// UI模式额外等待ui-finished（最长15分钟）；ui-card-management升级本隔离项目；
/// ui-project-navigation增加第二个只读项目，复用node_id=1且默认关闭，验证看板订阅切换。
/// ui-catalog-offline关闭目录响应，ui-catalog-empty暂时清空测试手机项目授权；移除标记后恢复原只读范围。
/// event.apns用于simctl本地注入，不表示真实APNs送达。
/// 预期结果：原生测试通过，配对未升权；结束后无有效测试授权；仅指定窗口/回合可被通知定位。
#[test]
#[ignore = "requires explicit native iOS runner and isolated simulator"]
fn native_notification_lifecycle_and_ui_fixture() {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("显式原生runner");
    let interactive = std::env::var_os("NORMA_NOTIFICATION_UI").is_some();
    let mode = if interactive {
        "notification-ui"
    } else {
        "notifications"
    };
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "通知验收项目")
        .expect("隔离项目");
    let second_path = dir.path().join("secondary");
    fs::create_dir(&second_path).expect("第二项目目录");
    let second = store
        .workspace_identity_store()
        .resolve_or_create(&second_path, "第二验收项目")
        .expect("第二隔离项目");
    let completed_card = CardSnapshot {
        node_id: 1,
        profile: "Codex".into(),
        title: "通知回源验收卡片".into(),
        mobile_control: true,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: None,
        state: "done".into(),
        run_id: Some("notification-run".into()),
        session_id: Some("notification-session".into()),
        turn_id: Some("notification-turn".into()),
        task_title: Some("本地通知定位验收".into()),
    };
    if interactive {
        // 两份真实持久化的合成计划：先旧后新，验证指挥台按序号读取最新计划，不调用模型或执行Agent。
        let plans = storage::ProjectPlanStore::new(store.path().into());
        let mut draft = agent::ProjectPlanDraft {
            summary: "旧工作简报".into(),
            todos: vec![agent::PlanTodo {
                target: completed_card.clone(),
                title: "核对连接路径".into(),
                instruction: "记录局域网与 Tailscale 使用的连接路径，再由用户决定下一步。".into(),
            }],
        };
        plans
            .publish(
                "overview-old",
                &project.workspace_id,
                "notification-route",
                "旧方向",
                &draft,
            )
            .expect("旧计划");
        draft.summary = "先核对连接路径，再整理实际测量结果。".into();
        plans
            .publish(
                "overview-current",
                &project.workspace_id,
                "notification-route",
                "优先解决手机连接延迟",
                &draft,
            )
            .expect("当前计划");
    }
    let identity = ServerIdentity::generate().expect("随机身份");
    let machine = identity.machine_id();
    let (source, snapshots) = async_channel::bounded::<LiveSnapshotRequest>(8);
    let (sources, mut decisions) = decision_ui::Fixture::sources(source);
    let (sources, mut creation) = card_ui::Fixture::sources(sources, dir.path());
    let (sources, mut todos) = todo_ui::Fixture::sources(sources);
    let (sources, mut directions) = direction_ui::Fixture::sources(sources);
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        if std::env::var_os("NORMA_MULTIROUTE_UI").is_some() {
            // 两条可用回环路径验证UI并行探路；不是物理LAN/Tailscale性能结论。
            vec![
                "127.0.0.1:0".parse().expect("路径A"),
                "127.0.0.1:0".parse().expect("路径B"),
            ]
        } else {
            vec!["127.0.0.1:0".parse().expect("回环")]
        },
        sources,
    )
    .expect("真实WSS服务");
    let offer = server.begin_pairing("通知验收 Mac").expect("签发隔离QR");
    let qr = dir.path().join("offer.json");
    fs::write(&qr, b"").expect("创建");
    fs::set_permissions(&qr, fs::Permissions::from_mode(0o600)).expect("QR仅自己可读");
    fs::write(&qr, offer.to_json().expect("QR编码")).expect("保存QR");
    let event = json!({"aps":{"alert":{"title":"Norma","body":"一项验收任务已完成"},"sound":"default"},
        "norma":{"version":1,"event_id":"11111111-2222-4333-8444-555555555555","kind":"completed",
        "machine_id":machine,"workspace_id":project.workspace_id,"route_id":"notification-route",
        "node_id":"1","run_id":"notification-run","session_id":"notification-session","turn_id":"notification-turn"}});
    fs::write(dir.path().join("event.apns"), event.to_string()).expect("本地通知定位fixture");
    let log = dir.path().join("native.log");
    let mut child = Command::new(probe)
        .arg(&qr)
        .arg(mode)
        .stdout(fs::File::create(&log).expect("原生日志"))
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("原生测试");
    // 180s = 120s配对期限 + 60s Xcode收尾；交互模式900s = 15min明确验收上限。
    let deadline = Instant::now() + Duration::from_secs(if interactive { 900 } else { 180 });
    let mut approved = None;
    let mut native_success = false;
    let mut announced = false;
    let mut revoked = false;
    let mut management = false;
    let mut navigation = false;
    let mut live_plan = false;
    let mut empty_scope = false;
    loop {
        if interactive {
            if let Some(directions) = &mut directions {
                directions.advance(dir.path(), &project.workspace_id, &completed_card);
            }
            if let Some(todos) = &mut todos {
                todos.advance(dir.path(), &project.workspace_id, &completed_card);
            }
            delivery_ui::advance(dir.path(), &project.workspace_id);
            if let Some(decisions) = &mut decisions {
                decisions.advance(dir.path(), &project.workspace_id);
            }
            if let Some(creation) = &mut creation {
                creation.advance(dir.path(), &project.workspace_id);
            }
        }
        if interactive && approved.is_some() {
            let empty = dir.path().join("ui-catalog-empty").exists();
            if empty != empty_scope {
                assert!(!navigation && !management, "空目录验收只使用初始只读项目");
                let grants = server.devices().expect("隔离授权");
                let grant = grants.first().expect("已配对测试手机");
                let scope = if empty {
                    vec![]
                } else {
                    vec![MobileProjectAccess {
                        workspace_id: project.workspace_id.clone(),
                        access: MobilePermission::Read,
                    }]
                };
                server
                    .replace_permissions(&grant.device_id, grant.revision, &scope)
                    .expect("切换隔离只读授权");
                empty_scope = empty;
                let applied = dir.path().join("ui-catalog-empty-applied");
                if empty {
                    fs::write(applied, b"").expect("空授权已生效");
                } else {
                    fs::remove_file(applied).expect("恢复原项目授权");
                }
            }
        }
        while let Ok(mut request) = snapshots.try_recv() {
            if interactive
                && !request.include_cards
                && dir.path().join("ui-catalog-offline").exists()
            {
                continue; // 仅关闭测试目录响应，正式服务会返回unavailable；不等待或伪造网络成功。
            }
            for p in &mut request.projects {
                if p.workspace_id == second.workspace_id {
                    let open = dir.path().join("ui-secondary-open").exists();
                    p.state = if open { "open" } else { "closed" }.into();
                    p.route_id = open.then(|| "secondary-route".into());
                    p.card_count = open.then_some(1);
                    p.cards = (request.include_cards && open).then(|| {
                        let mut card = completed_card.clone();
                        card.title = "第二项目原卡".into();
                        card.run_id = Some("secondary-run".into());
                        card.session_id = Some("secondary-session".into());
                        vec![card]
                    });
                    continue;
                }
                let unavailable = interactive && dir.path().join("ui-project-unavailable").exists();
                p.state = if unavailable { "unavailable" } else { "open" }.into();
                p.route_id = Some(
                    if dir.path().join("ui-stale").exists() {
                        "replacement-route"
                    } else {
                        "notification-route"
                    }
                    .into(),
                );
                let mut created = creation
                    .as_ref()
                    .map_or_else(Vec::new, |fixture| fixture.cards());
                if let Some(decisions) = &decisions {
                    created.extend(decisions.cards(dir.path()));
                }
                p.card_count =
                    (!unavailable).then_some(if interactive { 2 + created.len() } else { 1 });
                if request.include_cards
                    && !unavailable
                    && dir.path().join("ui-decision-window-replaced").exists()
                {
                    p.route_id = Some("replacement-route".into());
                }
                if unavailable {
                    p.route_id = None;
                }
                p.cards = (request.include_cards && !unavailable).then(|| {
                    let mut card = completed_card.clone();
                    if interactive && dir.path().join("ui-task-running").exists() {
                        card.state = "thinking".into();
                    }
                    if interactive && dir.path().join("ui-task-timing").exists() {
                        // Synthetic UI evidence: measured runtime timing is covered by mobile_native_card_tests.
                        card.task_timing = Some(norma::mobile::TaskTiming {
                            started_at_unix_ms: 1000,
                            completed_elapsed_ms: Some(60 * 1000), // one minute, frozen regardless of phone clock
                        });
                    }
                    if interactive {
                        // Explicit synthetic UI text, transported by the production WSS projection.
                        card.task_summary =
                            fs::read_to_string(dir.path().join("ui-task-summary")).ok();
                        if let Ok(value) = fs::read_to_string(dir.path().join("ui-task-progress")) {
                            // Synthetic layout evidence only; real Claude Task tools have a separate probe.
                            card.task_progress = Some(norma::mobile::TaskProgress {
                                completed: if value == "done" { 2 } else { 1 },
                                total: 2,
                                current_step: if value == "done" {
                                    None
                                } else {
                                    Some("Checking beta".into())
                                },
                            });
                        }
                    }
                    let mut cards = vec![card];
                    if interactive {
                        cards.push(CardSnapshot {
                            node_id: 2,
                            profile: "Claude".into(),
                            title: "空白验收卡片".into(),
                            mobile_control: false,
                            can_interrupt: None,
                            task_timing: None,
                            task_progress: None,
                            task_summary: None,
                            state: "idle".into(),
                            run_id: None,
                            session_id: None,
                            turn_id: None,
                            task_title: None,
                        });
                        cards.extend(created);
                    }
                    cards
                });
            }
            let _ = request.reply.send(request.projects);
        }
        if let Some(pending) = server.pending().expect("读取配对请求") {
            assert!(approved.is_none(), "不能覆盖或重复批准测试手机");
            server
                .approve(
                    &pending.request_id,
                    &[MobileProjectAccess {
                        workspace_id: project.workspace_id.clone(),
                        access: MobilePermission::Read,
                    }],
                )
                .expect("仅批准隔离只读项目");
            approved = Some(pending.device_id);
        }
        if !revoked && dir.path().join("native-done").exists() {
            server
                .revoke(approved.as_deref().expect("先配对"))
                .expect("测试撤销");
            revoked = true;
        }
        if interactive && !management && dir.path().join("ui-card-management").exists() {
            server
                .replace_permissions(
                    approved.as_deref().expect("先配对隔离手机"),
                    1, // 初始配对revision=1；本测试只升级一次，结束时仍撤销同一授权。
                    &[MobileProjectAccess {
                        workspace_id: project.workspace_id.clone(),
                        access: MobilePermission::Control,
                    }],
                )
                .expect("仅升级隔离测试项目");
            management = true;
        }
        if interactive && !navigation && dir.path().join("ui-project-navigation").exists() {
            assert!(!management, "导航验收不得和提权验收混用");
            server
                .replace_permissions(
                    approved.as_deref().expect("先配对隔离手机"),
                    1, // 单次从初始revision=1增加第二个只读项目，不扩大写权限。
                    &[
                        MobileProjectAccess {
                            workspace_id: project.workspace_id.clone(),
                            access: MobilePermission::Read,
                        },
                        MobileProjectAccess {
                            workspace_id: second.workspace_id.clone(),
                            access: MobilePermission::Read,
                        },
                    ],
                )
                .expect("仅增加隔离只读项目");
            navigation = true;
        }
        if interactive && !live_plan && dir.path().join("ui-live-plan-request").exists() {
            storage::ProjectPlanStore::new(dir.path().join("app.sqlite"))
                .publish(
                    "overview-live",
                    &project.workspace_id,
                    "notification-route",
                    "下一步验证双端输入",
                    &agent::ProjectPlanDraft {
                        summary: "连接已核对，接下来验证手机与电脑操作同一张卡片。".into(),
                        todos: vec![agent::PlanTodo {
                            target: completed_card.clone(),
                            title: "验证同一会话输入".into(),
                            instruction: "核对同一张卡片的两端输入与结果，由用户验收后继续。"
                                .into(),
                        }],
                    },
                )
                .expect("模拟另一端发布新计划，不执行Agent");
            live_plan = true;
            fs::write(dir.path().join("ui-live-plan-published"), b"").expect("记录发布完成");
        }
        if !native_success {
            if let Some(status) = child.try_wait().expect("测试进程状态") {
                assert!(
                    status.success(),
                    "原生通知测试失败：{}",
                    fs::read_to_string(&log).unwrap_or_default()
                );
                native_success = true;
            }
        }
        if native_success && interactive && !announced {
            assert!(
                dir.path().join("ui-ready").exists(),
                "不能把跳过测试当作配对准备完成"
            );
            println!("NOTIFICATION_UI_FIXTURE={}", dir.path().display());
            announced = true;
        }
        if native_success && (!interactive || dir.path().join("ui-finished").exists()) {
            break;
        }
        if Instant::now() >= deadline {
            if !native_success {
                child.kill().expect("停止超时测试");
                child.wait().expect("回收");
            }
            panic!(
                "通知验收超过有界等待：{}",
                fs::read_to_string(&log).unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(20)); // 与WSS listener一致，仅测试fixture轮询。
    }
    child.wait().expect("回收原生测试进程");
    if !revoked {
        server
            .revoke(approved.as_deref().expect("已配对"))
            .expect("回收测试授权");
    }
    assert!(
        server.devices().expect("授权").iter().all(|g| g.revoked),
        "不能残留测试授权"
    );
    let output = fs::read_to_string(log).expect("原生日志");
    let expected = if interactive {
        "NATIVE_NOTIFICATION_UI_READY"
    } else {
        "NATIVE_NOTIFICATION_LIFECYCLE_OK"
    };
    assert!(
        output.contains(expected),
        "必须执行真实原生检查，不能因跳过而通过：{output}"
    );
    server.shutdown();
}

use super::*;
use brain::acp::gate::{Gate, PermissionRequest, Verdict};
use brain::acp::trace::{AcpTrace, TraceItem, TraceKind};
use brain::{BrainStatusKind, Trigger};

fn scope(name: &str) -> BrainScope {
    BrainScope {
        workspace_id: format!("workspace-{name}"),
        route_id: format!("route-{name}"),
    }
}

/// 多项目共享一个宿主，客户端只领取自己的状态；最后一个项目释放使宿主及读取授权失效。
/// 数据：同一个owner注册alpha/beta两个根；两项目争取启动；完成一次不含外部进程的宿主。
/// 执行：向共享状态流发送beta trace，由alpha先轮询；逐个drop客户端并检查Gate实际文件读取。
/// 预期：仅一个启动ticket，统一token，beta状态不被alpha吞掉；释放alpha不关宿主，最后释放撤销全部读取。
#[test]
fn one_owner_routes_status_and_lives_until_last_project_is_released() {
    let dir = tempfile::tempdir().expect("隔离项目根");
    let alpha_root = dir.path().join("alpha");
    let beta_root = dir.path().join("beta");
    for root in [&alpha_root, &beta_root] {
        std::fs::create_dir(root).unwrap();
        std::fs::write(root.join("notes.md"), "project notes").unwrap();
    }
    let owner = SharedBrain::default();
    owner.configure(
        config::BrainSettings {
            enabled: true,
            ..Default::default()
        },
        true,
    );
    let alpha = owner.register(scope("alpha"), alpha_root.clone());
    let beta = owner.register(scope("beta"), beta_root.clone());
    let ticket = owner
        .begin_start(Instant::now())
        .expect("首项目取得唯一启动资格");
    assert!(
        owner.begin_start(Instant::now()).is_none(),
        "另一个项目不能同时启动第二个宿主"
    );
    let (tx, status) = std::sync::mpsc::channel();
    owner.complete_start(
        ticket,
        Ok(BrainHost {
            service: None,
            status,
            token: "global-token".into(),
        }),
    );
    assert_eq!(
        alpha.token().as_deref(),
        Some("global-token"),
        "alpha引用同一个token"
    );
    assert_eq!(beta.token(), alpha.token(), "两个项目共用宿主");
    tx.send(BrainStatus {
        scope: scope("beta"),
        kind: BrainStatusKind::Trace(AcpTrace {
            at: std::time::SystemTime::now(),
            lane: "Main".into(),
            turn: 1,
            trigger: Trigger::unknown(),
            item: TraceItem::new(TraceKind::Prompt, "", "beta"),
        }),
    })
    .unwrap();
    assert!(alpha.try_recv().is_none(), "alpha先poll也不能消费beta状态");
    let received = beta.try_recv().expect("beta收到原状态");
    assert_eq!(received.status.scope, scope("beta"), "状态保持完整身份");
    assert!(!received.retired, "当前服务回执非退休回执");
    let gate = Gate::new("/opt/norma".into(), dir.path().into(), owner.read_roots());
    let read = |root: &std::path::Path| PermissionRequest {
        tool: "Read".into(),
        command: None,
        path: Some(root.join("notes.md").display().to_string()),
    };
    assert_eq!(
        gate.judge(&read(&alpha_root)),
        Verdict::Allow,
        "已开alpha可读"
    );
    assert_eq!(
        gate.judge(&read(&beta_root)),
        Verdict::Allow,
        "已开beta可读"
    );
    drop(alpha);
    assert!(beta.is_running(), "关一个项目不能终止另一项目的全局幕僚");
    assert!(
        matches!(gate.judge(&read(&alpha_root)), Verdict::Reject(_)),
        "释放alpha即撤销读取"
    );
    assert_eq!(
        gate.judge(&read(&beta_root)),
        Verdict::Allow,
        "beta读取仍有效"
    );
    drop(beta);
    assert!(!owner.is_running(), "最后项目释放后全局宿主关闭");
    assert!(
        matches!(gate.judge(&read(&beta_root)), Verdict::Reject(_)),
        "最后项目的读取也立即撤销"
    );
    assert!(
        owner.begin_start(Instant::now()).is_none(),
        "零项目不能自动重启"
    );
}

/// 启动任务晚于最后项目释放返回，不能复活旧服务或与新启动重叠。
/// 数据：alpha取得启动ticket后释放，beta随后注册；旧任务此时尚未返回。
/// 执行：先请求新启动，再交付旧结果，最后交付beta的新结果。
/// 预期：旧任务收口前无第二个启动；旧令牌无效，新服务可正常启动。
#[test]
fn late_start_waits_for_shutdown_before_starting_a_new_owner_generation() {
    let owner = SharedBrain::default();
    owner.configure(
        config::BrainSettings {
            enabled: true,
            ..Default::default()
        },
        true,
    );
    let alpha = owner.register(scope("alpha"), "/alpha".into());
    let old = owner.begin_start(Instant::now()).expect("alpha启动");
    drop(alpha);
    let beta = owner.register(scope("beta"), "/beta".into());
    assert!(
        owner.begin_start(Instant::now()).is_none(),
        "旧后台启动尚未结束，不能创建并行宿主"
    );
    owner.complete_start(old, Ok(fixture_host("old")));
    assert!(!beta.is_running(), "旧启动结果不能复活");
    assert!(beta.token().is_none(), "旧令牌立即无效");
    let new = owner
        .begin_start(Instant::now())
        .expect("旧任务已收口，新代可以启动");
    assert_ne!(new, old, "启动身份必须更新");
    owner.complete_start(new, Ok(fixture_host("new")));
    assert_eq!(beta.token().as_deref(), Some("new"), "新代正常安装");
}

/// 语音开关不取消启动；多项目重放相同配置只能触发一次重启。
/// 数据：一个客户端、一份启用设置及后台启动ticket。
/// 执行：启动中仅改speak，然后交付host；更换策略，并重放相同配置。
/// 预期：第一份host有效；需要重启时令牌失效，相同配置不撤销新的启动ticket。
#[test]
fn settings_changes_restart_once_and_speaking_preserves_pending_start() {
    let owner = SharedBrain::default();
    let mut settings = config::BrainSettings {
        enabled: true,
        ..Default::default()
    };
    owner.configure(settings.clone(), true);
    let client = owner.register(scope("alpha"), "/alpha".into());
    let start = owner.begin_start(Instant::now()).expect("启动ticket");
    settings.speak = false;
    owner.configure(settings.clone(), false);
    owner.complete_start(start, Ok(fixture_host("first")));
    assert_eq!(
        client.token().as_deref(),
        Some("first"),
        "语音开关不取消启动"
    );
    settings.strategy = brain::Strategy::Quality;
    owner.configure(settings.clone(), true);
    assert!(client.token().is_none(), "重启立即撤销旧令牌");
    let restart = owner.begin_start(Instant::now()).expect("停止后重启");
    owner.configure(settings, true);
    owner.complete_start(restart, Ok(fixture_host("second")));
    assert_eq!(
        client.token().as_deref(),
        Some("second"),
        "其他项目重放配置不能再次取消启动"
    );
}

fn fixture_host(token: &str) -> BrainHost {
    BrainHost {
        service: None,
        status: mpsc::channel().1,
        token: token.into(),
    }
}

/// App项目实体必须订阅同一个Global，真正停止项目才撤销订阅。
/// 数据：同一TestAppContext内两个独立项目Entity，共享App Global和状态流。
/// 执行：生产ensure注册、安装一次host、由alpha先排空，再停止两个项目。
/// 预期：两个项目令牌相同、beta状态完整；停止alpha不影响beta，最后停止清空权限。
#[test]
fn app_projects_use_the_global_owner_and_release_it_on_stop() {
    use crate::app::MultisoulApp;
    use gpui::{AppContext, TestAppContext};
    let dir = tempfile::tempdir().expect("隔离App目录");
    let mut test = TestAppContext::single();
    let make = |test: &mut TestAppContext| {
        test.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                storage::DatabasePaths::isolated_test(dir.path()),
            )
        })
    };
    let alpha = make(&mut test);
    let beta = make(&mut test);
    for (entity, name) in [(&alpha, "alpha"), (&beta, "beta")] {
        entity.update(&mut test, |app, cx| {
            app.workspace_id = Some(scope(name).workspace_id);
            app.workspace_route_id = Some(scope(name).route_id);
            app.runtime.set_workspace_root(dir.path().join(name));
            app.ensure_brain_client(cx);
            app.apply_brain_settings(
                config::BrainSettings {
                    enabled: true,
                    ..Default::default()
                },
                true,
                cx,
            );
        });
    }
    let owner = test.read(|cx| cx.global::<SharedBrain>().clone());
    let ticket = owner.begin_start(Instant::now()).expect("唯一启动");
    let (tx, status) = mpsc::channel();
    owner.complete_start(
        ticket,
        Ok(BrainHost {
            service: None,
            status,
            token: "shared".into(),
        }),
    );
    for entity in [&alpha, &beta] {
        assert!(
            entity.read_with(&test, |app, _| app.brain.token_matches("shared")),
            "每个项目都引用全局服务"
        );
    }
    tx.send(BrainStatus {
        scope: scope("beta"),
        kind: BrainStatusKind::Trace(AcpTrace {
            at: std::time::SystemTime::now(),
            lane: "Main".into(),
            turn: 31,
            trigger: Trigger::unknown(),
            item: TraceItem::new(TraceKind::Prompt, "", "beta"),
        }),
    })
    .unwrap();
    alpha.update(&mut test, |app, cx| app.drain_brain_status(cx));
    beta.update(&mut test, |app, cx| app.drain_brain_status(cx));
    assert!(
        alpha.read_with(&test, |app, _| app.brain.last_turn.is_empty()),
        "alpha不能领取beta状态"
    );
    assert_eq!(
        beta.read_with(&test, |app, _| app.brain.last_turn.get("Main").copied()),
        Some(31),
        "beta接到精确状态"
    );
    alpha.update(&mut test, |app, cx| app.stop_project(cx));
    assert!(
        beta.read_with(&test, |app, _| app.brain.token_matches("shared")),
        "另一项目仍运行"
    );
    assert!(
        !alpha.read_with(&test, |app, _| app.brain.token_matches("shared")),
        "已停止项目不再授权"
    );
    beta.update(&mut test, |app, cx| app.stop_project(cx));
    assert!(!owner.is_running(), "最后项目停止后无服务");
}

/// 真实服务启动后释放一个客户端，必须取消该项目的在途请求，保留项目继续执行。
/// 数据：一个共享owner、alpha/beta客户端、真实BrainService与脚本ACP子进程。
/// 执行：alpha占住主lane直到收到实际Prompt，drop alpha后提交beta；最后释放beta并等待后台join。
/// 预期：无需释放alpha的文件闸门，beta仍完成；最后项目释放使服务线程和子进程收口。
#[test]
fn releasing_a_client_cancels_its_real_service_work_and_keeps_other_clients_alive() {
    let dir = tempfile::tempdir().expect("隔离真实服务夹具");
    let owner = SharedBrain::default();
    owner.configure(
        config::BrainSettings {
            enabled: true,
            ..Default::default()
        },
        true,
    );
    let alpha = owner.register(scope("alpha"), dir.path().join("alpha"));
    let beta = owner.register(scope("beta"), dir.path().join("beta"));
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../brain/tests/support/fake_acp.py");
    let mut service_config = brain::BrainConfig::new(
        brain::Backend {
            kind: brain::BackendKind::ClaudeCode,
            program: "python3".into(),
            args: vec![script.display().to_string()],
        },
        "/opt/norma/bin/norma".into(),
        dir.path().into(),
        dir.path().join("global"),
    );
    service_config.router.summary_every = None;
    service_config.read_roots = owner.read_roots();
    let log = dir.path().join("prompt.log");
    service_config.env = vec![
        ("FAKE_LOG".into(), log.display().to_string()),
        ("CLAUDE_CONFIG_DIR".into(), dir.path().display().to_string()),
        (
            "FAKE_RELEASE".into(),
            dir.path().join("never-release").display().to_string(),
        ),
    ];
    let ticket = owner.begin_start(Instant::now()).expect("唯一服务启动");
    let (service, status) = brain::BrainService::start(service_config);
    owner.complete_start(
        ticket,
        Ok(BrainHost {
            service: Some(service),
            status,
            token: "real-shared".into(),
        }),
    );
    assert!(
        alpha
            .push(
                brain::BrainEventKind::UserRequest {
                    text: "FAKE_HOLD alpha".into()
                }
                .in_scope(scope("alpha"))
            )
            .is_none(),
        "alpha事件实际交给服务"
    );
    let prompted = wait_for_client_status(
        &alpha,
        |status| matches!(&status.kind, BrainStatusKind::Trace(trace) if trace.item.kind == TraceKind::Prompt),
    );
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    let received = loop {
        if std::fs::read_to_string(&log).is_ok_and(|text| {
            text.lines()
                .any(|line| line.starts_with("prompt:") && line.contains("FAKE_HOLD alpha"))
        }) {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    drop(alpha);
    assert!(beta.is_running(), "alpha释放后beta宿主保持运行");
    assert!(
        beta.push(
            brain::BrainEventKind::UserRequest {
                text: "beta continues".into()
            }
            .in_scope(scope("beta"))
        )
        .is_none(),
        "beta事件仍可投递"
    );
    let completed = wait_for_client_status(
        &beta,
        |status| matches!(&status.kind, BrainStatusKind::Trace(trace) if trace.item.end.as_ref().is_some_and(|end| end.stop_reason == "end_turn")),
    );
    drop(beta);
    // Always clean up the real service before asserting, including the expected RED path.
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    while !owner.0.borrow().retired.is_empty() && Instant::now() < deadline {
        owner.0.borrow_mut().poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        owner.0.borrow().retired.is_empty(),
        "实际后台shutdown已完成并排空状态"
    );
    assert!(received, "子进程日志证明实际收到alpha请求");
    assert!(prompted, "alpha真实进入在途状态，不能靠没启动假通过");
    assert!(
        completed,
        "drop alpha必须取消占用主lane的任务，让beta真实完成"
    );
}

fn wait_for_client_status(client: &BrainClient, predicate: impl Fn(&BrainStatus) -> bool) -> bool {
    // Five seconds bounds a local scripted process; each iteration observes an actual status.
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    while Instant::now() < deadline {
        if client
            .try_recv()
            .is_some_and(|entry| predicate(&entry.status))
        {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

use crate::harness::make_app_real;
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;

/// 原生手机从空Codex卡开始真实任务、读回正文、继续同一会话，并拒绝旧快照重放。
///
/// 数据构造：一个隔离项目/空卡、含LF/Tab的无工具固定回复提示，两次独立operation ID。
/// 执行过程：空卡 → 原生测试配对control → 草稿落盘/重读 → 首条发送/回执/清草稿 → 续发 → 重复submit → 关闭。
/// 预期结果：Provider只收到两条完整原文，准确回执/起点落盘；原生终端保留；旧卡快照拒绝。
#[test]
#[ignore = "requires real Codex, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_codex_messages_native() {
    verify(AgentProfile::Codex, "messages");
}

/// 原生手机从空Claude卡开始真实任务，通过准确PromptSubmit确认后续消息。
///
/// 数据构造：已信任仓库cwd，App数据库与配对临时；两条含LF/Tab的提示禁止工具和文件读取/修改。
/// 执行过程：保留原生PTY → 草稿落盘/重读 → Ready → 发送/准确Hook/清草稿 → 第二条 → 重放回查 → 关闭。
/// 预期结果：LF与首尾空白保留，仅Tab按Claude转换为4空格；不提前提交或产生第三条；关窗控制失效。
#[test]
#[ignore = "requires real Claude, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_claude_messages_native() {
    verify(AgentProfile::Claude, "messages");
}

/// 原生手机准确批准/拒绝真实 Codex 工具，并在丢失响应后查询原回执。
///
/// 数据构造：正式空卡，独立临时目录中的 denied.txt/allowed.txt，只写固定问候文本。
/// 执行过程：control 配对 → 手机发送任务 → 查询请求 → 先拒绝再批准 → 重试同操作 → 查询历史。
/// 预期结果：拒绝文件不存在，批准文件内容准确；请求版本不复用；保留原生 PTY 与持久化回执。
#[test]
#[ignore = "requires real Codex, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_codex_approvals_native() {
    verify(AgentProfile::Codex, "approvals");
}

/// 手机显示真实 Codex 外部等待，但不能代替扩展审批；用户仍可停止该回合。
///
/// 数据构造：保留当前外部 Hook 的正式 Codex，精确临时写入请求，无自动批准。
/// 执行过程：手机派发 → 外部 Hook 开始 → WSS 只读通知 → 拒绝构造批准 → 精确停止。
/// 预期结果：无配置路径/命令泄漏，没有手机批准选项；停止回执真实且文件未创建。
#[test]
#[ignore = "requires real Codex with blocking external Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_codex_external_notice_native() {
    verify(AgentProfile::Codex, "external");
}

/// 原生手机处理真实 Claude 请求级审批，不能以选择或 Hook 入队冒充执行成功。
///
/// 数据构造：已信任仓库 cwd，隔离数据库/写入目录，仅准许明确的 printf 单次命令。
/// 执行过程：手机发送 → 完整工具输入匹配 → 拒绝/批准 → 回执回查 → 真实回合结束/历史读取。
/// 预期结果：批准只作用于当前请求；原操作重放不执行两次，拒绝目标不写入。
#[test]
#[ignore = "requires real Claude, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_claude_approvals_native() {
    verify(AgentProfile::Claude, "approvals");
}

/// 真实Codex忙碌时将Todo入队并撤回，停止原生成后重新安排，拒绝项不进入对话。
///
/// 数据构造：复用prime无工具长生成，三条短Todo为计划fixture；决定/入队/撤回不能预写数据库。
/// 执行过程：原生入队/撤回 → 确认cancelled/可重排 → 停止原生成 → 重排首项/拒绝另一项/队列末项。
/// 预期结果：原生成之外仅两条Todo原文/真实turn；重排准确关联，原入队回执和撤回状态保留。
/// 范围：Provider、原撤回、后续派发/结果均为正式路径；计划内容仍是fixture，非云规划或SwiftUI点击验收。
#[test]
#[ignore = "requires real Codex, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_codex_todos_native() {
    verify(AgentProfile::Codex, "todos");
}

/// 真实Claude使用同一Todo流程，不能把Hook输入回执误当作实际任务完成。
///
/// 数据构造：已信任cwd、独立AppDB/三项计划；prime真实生成期间通过手机入队并撤回，无预写失败。
/// 执行过程：入队/撤回 → 原生停止 → 重排/拒绝/队列 → 实际PromptSubmit和完整历史 → 旧操作重放。
/// 预期结果：保留原生成/系统中断消息，之后只有两个准确Todo回复和Stop；旧撤回不覆盖、不影响新回合。
#[test]
#[ignore = "requires real Claude, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_claude_todos_native() {
    verify(AgentProfile::Claude, "todos");
}

/// 原有Claude卡接受计划待办，无需重开或迁移为受控模式。
///
/// 数据构造：mobile_control=false原卡、一个真实长回合、三项既有Todo fixture；不执行工具。
/// 执行过程：原生生成 → 原生Swift入队/撤回 → 停止 → 重排/拒绝/续发 → 核对实际终态。
/// 预期结果：原run/session及唯一PTY保留；仅两项被派发，拒绝项无执行记录，不能创建新执行器。
#[test]
#[ignore = "requires real original Claude card and NORMA_SWIFT_PROBE"]
fn e2e_original_claude_todos_native() {
    verify_mode(AgentProfile::Claude, "todos", true);
}

/// 原有Codex卡的Todo入队、撤回与重排必须继续使用同一会话。
///
/// 数据构造：一张原卡、一个无工具长回合、三项Todo；只允许首项和末项各执行一次。
/// 执行过程：忙碌时入队→撤回→停止→重排首项→拒绝中项→执行末项→查询历史与回执。
/// 预期结果：两个准确Todo回合存在，拒绝项不存在；原run/session/唯一PTY和原卡模式保留。
#[test]
#[ignore = "requires original Codex and NORMA_SWIFT_PROBE"]
fn e2e_original_codex_todos_native() {
    verify_mode(AgentProfile::Codex, "todos", true);
}

fn verify(profile: AgentProfile, mode: &str) {
    verify_mode(profile, mode, false);
}

fn verify_mode(profile: AgentProfile, mode: &str, original: bool) {
    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("显式原生客户端");
    let mut test = TestAppContext::single();
    let directory = tempfile::tempdir().expect("隔离路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(directory.path().join("norma.sock"), cx)
            .expect("正式路由")
    });
    let (app, cx) = make_app_real(&mut test);
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("workspace");
    let feedback_directory = (mode == "feedback").then(|| {
        tempfile::Builder::new()
            .prefix(".norma-feedback-")
            .tempdir_in(workspace)
            .expect("已信任仓库中的隔离授权子目录")
    });
    let workspace = feedback_directory
        .as_ref()
        .map_or(workspace, |dir| dir.path());
    if mode == "feedback" {
        //Own Git metadata avoids inheriting the parent repository's unsupported LFS filters.
        //Only initialize this disposable directory; never edit the user's Git configuration.
        let status = std::process::Command::new("/usr/bin/git")
            .args(["init", "--quiet"])
            .current_dir(workspace)
            .status()
            .expect("独立Git fixture");
        assert!(status.success(), "隔离Git初始化必须成功");
    }
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(workspace, window, cx)
        })
    });
    let node = app.update(cx, |app, _| {
        if original {
            return app
                .runtime
                .spawn_agent(profile, CanvasRect::new(0., 0., 640., 400.), 80, 24)
                .expect("原有原生卡，启动CLI不派任务");
        }
        let node = app
            .runtime
            .create_mobile_agent_card(profile, None, None, CanvasRect::new(0.0, 0.0, 640.0, 400.0))
            .expect("标准640×400空卡");
        assert_eq!(app.runtime.live_len(), 0, "创建不能立刻安排任务");
        node
    });
    if matches!(mode, "interrupts" | "queues" | "todos") {
        prime(&app, cx, node, profile, original);
    }
    let original_key = original.then(|| {
        app.read_with(cx, |app, _| {
            app.runtime.mobile_latest_turn_key(node).expect("原生回合")
        })
    });
    if mode == "todos" {
        let (database, identity, target) = app.read_with(cx, |app, _| {
            (
                app.app_database_path_for_e2e(),
                app.norma_workspace_identity_for_e2e().expect("项目身份"),
                app.runtime.mobile_card_snapshots().remove(0),
            )
        });
        let instruction =
            |name| format!("Do not use any tools, read or modify files. Reply exactly: {name}");
        let todos = [
            "NORMA_TODO_FIRST",
            "NORMA_TODO_DECLINED",
            "NORMA_TODO_SECOND",
        ]
        .into_iter()
        .map(|name| agent::PlanTodo {
            target: target.clone(),
            title: name.into(),
            instruction: instruction(name),
        })
        .collect();
        storage::ProjectPlanStore::new(database)
            .publish(
                "native-plan",
                &identity.0,
                &identity.1,
                "验证Todo派发",
                &agent::ProjectPlanDraft {
                    summary: "原生Todo验收".into(),
                    todos,
                },
            )
            .expect("准确计划");
    }
    crate::e2e_mobile_conversation::verify_native(&app, cx, workspace, &probe, mode);
    if mode == "messages" {
        let (database, identity, card) = app.read_with(cx, |app, _| {
            (
                app.app_database_path_for_e2e(),
                app.norma_workspace_identity_for_e2e()
                    .expect("原生测试项目"),
                app.runtime.mobile_card_snapshots().remove(0),
            )
        });
        // Completed cards can clear turn_id. The immutable sending receipts remain authoritative.
        let db = rusqlite::Connection::open(&database).expect("仅查询隔离AppDB");
        let mut query = db.prepare("SELECT operation_id,result_json FROM mobile_commands WHERE workspace_id=?1 AND state='succeeded' ORDER BY rowid").expect("实际发送回执");
        let receipts: Vec<(String, String)> = query
            .query_map([&identity.0], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("查询")
            .collect::<Result<_, _>>()
            .expect("两条结果");
        assert_eq!(
            receipts.len(),
            2,
            "原生客户端只有两次成功发送，旧快照及重放不可生成第三条"
        );
        let baselines = storage::MobileTaskBaselineStore::new(database);
        let read = |receipt: &(String, String)| {
            let mut key: serde_json::Value = serde_json::from_str(&receipt.1).expect("完整回执");
            key["profile"] = serde_json::json!(card.profile);
            let key: storage::MobileTurnKey = serde_json::from_value(key).expect("真实回合身份");
            baselines
                .get(&identity.0, &key)
                .expect("落盘读取")
                .expect("原生实际回合必须保存起点或明确不可用")
        };
        let first = read(&receipts[0]);
        let second = read(&receipts[1]);
        assert_eq!(
            app.read_with(cx, |app, _| app.runtime.mobile_latest_turn_key(node)),
            Some(second.key.clone()),
            "最后实际回合必须与第二条发送回执一致"
        );
        assert_eq!(
            first.operation_id, receipts[0].0,
            "首轮起点必须属于第一次手机发送"
        );
        assert_eq!(
            second.operation_id, receipts[1].0,
            "续发起点必须属于第二次手机发送"
        );
        assert_eq!(second.route_id, identity.1, "续发起点属于真实窗口");
        assert!(
            first.key.turn_id != second.key.turn_id,
            "旧起点不能冒充续发任务的起点"
        );
    }
    app.update(cx, |app, _| {
        if let Some(key) = original_key {
            assert_eq!(app.runtime.live_len(), 1, "Todo执行必须使用唯一原PTY");
            assert_eq!(app.runtime.mobile_agent_run_id(node), Some(key.run_id.as_str()), "Todo不能重开运行");
            assert_eq!(app.runtime.agent_session_id(node).as_deref(), Some(key.session_id.as_str()), "Todo保留原会话");
            assert!(matches!(&app.runtime.document.node(node).expect("原卡").kind, canvas::CanvasNodeKind::Agent(spec) if !spec.mobile_control), "Todo不能改变原卡运行模式");
        }
        assert!(
            app.runtime.node_view(node).is_some(),
            "真实原生终端必须保留"
        );
        app.runtime.kill_all();
        assert!(
            app.runtime.mobile_agent_run_id(node).is_none(),
            "关闭后运行不可继续控制"
        );
    });
}

/// 原生手机停止真实 Codex 生成，断线回查同一回执，再向保留的终端继续发消息。
///
/// 数据构造：真实无工具长回答，Provider 已确认活动回合；唯一停止 ID 和一条短续发。
/// 执行过程：后台受控启动 → Provider 回合开始 → 原生 WSS 配对/control → 停止/查询/重放 → 同会话续发。
/// 预期结果：准确 interrupted 终态，旧请求不影响新回合；短消息真实完成且保留 PTY，关闭后不可控制。
#[test]
#[ignore = "requires real Codex, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_codex_interrupts_native() {
    verify(AgentProfile::Codex, "interrupts");
}

/// 原生手机中断真实 Claude 生成，只有新增 transcript 证据可完成，再验证同会话聊天。
///
/// 数据构造：已信任 cwd、独立 App 数据库与真实 CLI，无工具长回答；停止操作完整持久化。
/// 执行过程：PromptSubmit → 首行 READY → 原生请求 → EOF/系统 Escape/新记录 → 查询 → 新消息。
/// 预期结果：停止回执不伪造成功，不污染电脑草稿，续发准确到原 session，终端未被关闭。
#[test]
#[ignore = "requires real Claude, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_claude_interrupts_native() {
    verify(AgentProfile::Claude, "interrupts");
}

pub(crate) fn prime(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    node: canvas::NodeId,
    profile: AgentProfile,
    native: bool,
) {
    use crate::harness::drain_until;
    use std::time::Duration;
    let task = "Without tools, begin your reply with the five capital letters R E A D Y joined without spaces on their own line. Then write at least 10000 words explaining database transactions in detail.";
    if !native {
        let launch = app.update(cx, |app, cx| {
            if profile == AgentProfile::Codex {
                app.start_mobile_codex(node, task.into(), cx)
            } else {
                app.start_mobile_agent(node, cx)
            }
            .expect("正式启动")
        });
        // 30s is the controlled CLI cold-start budget.
        drain_until(app, cx, |_| launch.try_recv().ok(), Duration::from_secs(30))
            .expect("启动返回")
            .expect("受控启动成功");
    }
    if profile == AgentProfile::Codex && !native {
        // Codex turn/start is authoritative and interruptible immediately; no ANSI text inference.
        drain_until(
            app,
            cx,
            |r| {
                let card = r.mobile_card_snapshots().remove(0);
                (card.turn_id.is_some() && card.state == "thinking").then_some(())
            },
            Duration::from_secs(30),
        )
        .expect("真实 Codex 回合必须活动");
        return;
    }
    if profile == AgentProfile::Claude || native {
        drain_until(
            app,
            cx,
            |r| {
                r.mobile_card_snapshots()
                    .iter()
                    .any(|card| card.node_id == node && card.state == "ready")
                    .then_some(())
            },
            Duration::from_secs(30),
        )
        .expect("原生CLI Ready");
        app.update(cx, |app, _| {
            app.runtime
                .send_node_input(node, runtime::NodeInput::Text(task.into()))
                .expect("无工具任务")
        });
        // 100ms is the existing native paste-to-Enter separation.
        std::thread::sleep(Duration::from_millis(100));
        app.update(cx, |app, _| {
            app.runtime
                .send_node_input(
                    node,
                    runtime::NodeInput::Key(terminal::KeyInput {
                        key: terminal::Key::Enter,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    }),
                )
                .expect("提交")
        });
    }
    // 120s bounds model startup; 10000 words leaves time for the native XCTest host to attach.
    let ready = drain_until(
        app,
        cx,
        |r| {
            let runtime::NodeView::Term { snapshot, .. } = r.node_view(node)?;
            let text: String = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
            (text.contains("READY") && r.mobile_card_snapshots()[0].turn_id.is_some()).then_some(())
        },
        Duration::from_secs(120),
    );
    if ready.is_none() {
        app.update(cx, |app, _| app.runtime.kill_all());
    }
    assert!(
        ready.is_some(),
        "必须观察真实生成，不能仅凭 PromptSubmit 中断"
    );
}

/// 原生 Swift 客户端在真实 Codex 生成时排三条、撤回第二条，停止后由 Mac 自动按序发送。
///
/// 数据构造：隔离数据库与受控卡；一条无工具长回答、三条固定短回复；首尾两条需要实际发送。
/// 执行过程：活动回合 → 入队/草稿交接 → 撤回第二条 → 中断 → 重读本地记录 → 回查两次准确发送 → 重试旧入队。
/// 预期结果：仅首尾两条按序出现在真实对话；第二条不出现；旧入队回执不重放，原生终端保留。
#[test]
#[ignore = "requires real Codex, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_codex_queues_native() {
    verify(AgentProfile::Codex, "queues");
}

/// 真实 Claude 队列沿用原生输入与 PromptSubmit 确认，不把 Enter 写入当作已发送。
///
/// 数据构造：已信任 cwd、临时 App 数据库；活动长回复和三条含 LF/Tab 的排队消息。
/// 执行过程：原生客户端入队 → 安全撤回第二条 → 停止长回复 → Mac 派发首尾 → 新客户端读取准确回执与历史。
/// 预期结果：Tab 仅按 Claude 转为4空格；完整原文按序出现且第二条缺席，不串会话或重复执行。
#[test]
#[ignore = "requires real Claude, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_claude_queues_native() {
    verify(AgentProfile::Claude, "queues");
}

/// 真实Codex接收对冻结成果的原始修改意见，回执绑定原成果和同会话新turn。
///
/// 数据构造：已信任Git仓库内的隔离空目录，两轮无工具固定回复，另有关闭后禁止派发的意见。
/// 执行过程：首轮完成/自动交付 → 原生保存反馈草稿/发送/回查 → 第二轮完成 → 重放去重 → 关闭后拒绝。
/// 预期结果：仅两条真实消息；旧成果不变；反馈草稿独立清除，关闭后不能冷启动另一个Agent。
#[test]
#[ignore = "requires real Codex, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_codex_feedback_native() {
    verify(AgentProfile::Codex, "feedback");
}

/// 真实Claude使用相同成果反馈流程，保留原生PTY与准确PromptSubmit身份。
///
/// 数据构造：已信任仓库内空子目录，两个无工具短任务，意见含首尾空格和LF。
/// 执行过程：首轮真实完成 → 冻结交付 → Swift持久化/发送意见 → 原始正文/新回合/旧成果校验 → 关闭拒绝。
/// 预期结果：原始意见不trim；同会话仅新回合成功；旧回合无验收，普通聊天草稿不被清除。
#[test]
#[ignore = "requires real Claude, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_claude_feedback_native() {
    verify(AgentProfile::Claude, "feedback");
}

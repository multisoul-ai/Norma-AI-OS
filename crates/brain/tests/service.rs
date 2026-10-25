//! 用脚本化的假 ACP 适配器测试 BrainService 的完整链路。

use brain::acp::trace::{AcpTrace, TraceKind};
use brain::risk::ApprovalSubject;
use brain::{
    ApprovalRequest, Backend, BackendKind, BrainConfig, BrainEvent, BrainService, BrainStatus,
    RouterConfig,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

const NORMA: &str = "/opt/norma/bin/norma";

/// 所有后端都允许启动适配器；不再因隔离能力不足提前拒绝。
/// 数据构造：Claude/Codex/OpenCode 各给一个不存在的程序路径。
/// 预期结果：不存在“隔离”类的不可用原因。
#[test]
fn all_backends_are_allowed_to_start() {
    for kind in [
        BackendKind::ClaudeCode,
        BackendKind::Codex,
        BackendKind::OpenCode,
    ] {
        assert!(
            brain::backend::unavailable_reason(kind).is_none(),
            "{kind:?}"
        );
    }
}

struct Harness {
    service: BrainService,
    status: Receiver<BrainStatus>,
    log: PathBuf,
    _dir: tempfile::TempDir,
}

fn start() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("fake.log");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_acp.py");
    let backend = Backend {
        kind: BackendKind::ClaudeCode,
        program: "python3".into(),
        args: vec![script.display().to_string()],
    };
    let mut config = BrainConfig::new(
        backend,
        NORMA.into(),
        dir.path().to_path_buf(),
        dir.path().join("state"),
    );
    config.env = vec![("FAKE_LOG".into(), log.display().to_string())];
    config.retry_backoff = Duration::from_millis(50);
    config.router = RouterConfig {
        debounce: Duration::from_millis(50),
        quiet_window: Duration::from_millis(50),
        summary_every: None,
        norma_bin: String::new(),
    };
    let (service, status) = BrainService::start(config);
    Harness {
        service,
        status,
        log,
        _dir: dir,
    }
}

impl Harness {
    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn wait_status(&self, want: impl Fn(&BrainStatus) -> bool) -> BrainStatus {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.status.recv_timeout(left) {
                Ok(status) if want(&status) => return status,
                Ok(_) => {}
                Err(_) => panic!("等待状态超时，假适配器日志：\n{}", self.log()),
            }
        }
    }

    fn wait_log(&self, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !self.log().contains(needle) {
            assert!(
                Instant::now() < deadline,
                "日志里一直没有 {needle:?}：\n{}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

fn approval(id: &str, command: &str) -> BrainEvent {
    BrainEvent::ApprovalRequested(ApprovalRequest {
        id: id.into(),
        agent: "Claude".into(),
        subject: ApprovalSubject {
            tool_name: "Bash".into(),
            command: Some(command.into()),
            file_path: None,
            project_root: "/work/proj".into(),
        },
    })
}

/// 普通审批：审批会话照提示执行 brain decide，闸门放行，随后通知本轮结束。
///
/// 数据构造：审批 a1（cargo test）。假适配器看到提示里的 decide 命令后发起权限请求。
/// 预期结果：
///   - 正断言：日志出现 `allowed:` 加 brain decide；收到 ApprovalTurnEnded(a1)；
///     会话创建时带了 settingSources 空数组，切到 default 模式，并明确切到按策略选的模型
///   - 负断言：没有 Escalate
#[test]
fn approval_lane_runs_the_decide_command_through_the_gate() {
    let h = start();
    h.service.push(approval("a1", "cargo test"));
    h.wait_status(|s| matches!(s, BrainStatus::ApprovalTurnEnded { id } if id == "a1"));
    let log = h.log();
    assert!(
        log.contains(&format!("allowed: {NORMA} brain decide a1 allow")),
        "decide 命令应被放行：\n{log}"
    );
    assert!(
        log.contains("\"settingSources\": []") || log.contains("\"settingSources\":[]"),
        "要忽略用户设置：\n{log}"
    );
    assert!(
        log.contains("set_mode default"),
        "要切到 default 模式：\n{log}"
    );
    // 用户自己的 Claude Code 设置会覆盖 `_meta` 里的模型，所以建好会话后要再明确切一次。
    assert!(
        log.contains("set_config model=haiku"),
        "审批会话要明确切到 haiku：\n{log}"
    );
    assert!(!log.contains("rejected:"), "没有请求应被拒绝：\n{log}");
    h.service.shutdown();
}

/// `git push` 这类命令也交给大脑判断：提示发给了审批会话，并带着风险评估要求。
///
/// 数据构造：`git push origin main`。
/// 预期结果：
///   - 正断言：假适配器日志里有 a2 的提示，含“可逆”；本轮结束后收到 ApprovalTurnEnded
///   - 负断言：没有任何绕过大脑的直接转人工
#[test]
fn git_push_approval_is_judged_by_the_brain() {
    let h = start();
    h.service.push(approval("a2", "git push origin main"));
    h.wait_status(|s| matches!(s, BrainStatus::ApprovalTurnEnded { id } if id == "a2"));
    let log = h.log();
    assert!(
        log.contains("审批 a2") && log.contains("git push origin main"),
        "{log}"
    );
    assert!(log.contains("可逆"), "提示要带风险评估要求：{log}");
    h.service.shutdown();
}

/// 大脑想执行 norma 以外的命令时被闸门拒绝。
///
/// 数据构造：用户请求里带 `FAKE_RUN: rm -rf x` 和 `FAKE_RUN: <norma> brain agents`。
/// 预期结果：
///   - 正断言：norma 命令 allowed
///   - 负断言：rm 命令 rejected，不是 allowed
#[test]
fn the_gate_rejects_non_norma_commands_from_the_main_lane() {
    let h = start();
    h.service.push(BrainEvent::UserRequest {
        text: format!("FAKE_RUN: rm -rf x\nFAKE_RUN: {NORMA} brain agents"),
    });
    h.wait_log(&format!("allowed: {NORMA} brain agents"));
    let log = h.log();
    assert!(log.contains("rejected: rm -rf x"), "rm 应被拒绝：\n{log}");
    assert!(!log.contains("allowed: rm -rf x"), "rm 不应被放行：\n{log}");
    h.service.shutdown();
}

/// 适配器崩溃后报告 BackendDown，退避后能用新会话继续处理。
///
/// 数据构造：先发含 FAKE_CRASH 的请求让适配器退出，再发一条正常请求。
/// 预期结果：
///   - 正断言：收到 BackendDown；之后的请求被处理（日志里有第二条 prompt）
///   - 负断言：崩溃那条之后日志里 session/new 出现了第二次（说明重建了会话）
#[test]
fn a_crashed_adapter_is_reported_and_replaced() {
    let h = start();
    h.service.push(BrainEvent::UserRequest {
        text: "FAKE_CRASH".into(),
    });
    h.wait_status(|s| matches!(s, BrainStatus::BackendDown { .. }));
    h.service.push(BrainEvent::UserRequest {
        text: "第二条请求".into(),
    });
    h.wait_log("第二条请求");
    assert_eq!(
        h.log().matches("session/new").count(),
        2,
        "崩溃后应重建会话：\n{}",
        h.log()
    );
    h.service.shutdown();
}

/// 审批在排队期间被用户处理后，大脑不再判断它。
///
/// 数据构造：a3 的命令含 FAKE_SLEEP，占住审批会话 0.6 秒；排队中的 a4 随即被 ApprovalClosed。
/// 预期结果：
///   - 正断言：a3 完成（ApprovalTurnEnded）
///   - 负断言：日志里没有 a4 的提示
#[test]
fn approvals_closed_while_queued_are_skipped() {
    let h = start();
    h.service.push(approval("a3", "cargo test FAKE_SLEEP"));
    h.wait_log("a3");
    h.service.push(approval("a4", "cargo build"));
    h.service
        .push(BrainEvent::ApprovalClosed { id: "a4".into() });
    h.wait_status(|s| matches!(s, BrainStatus::ApprovalTurnEnded { id } if id == "a3"));
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        !h.log().contains("审批 a4"),
        "已关闭的审批不应再判断：\n{}",
        h.log()
    );
    h.service.shutdown();
}

/// 多条完成事件合并成一次主会话提示，且提示里带上已记录的目标；目标本身不单独触发一轮。
///
/// 数据构造：先记录目标，再在去抖窗口内连续完成两个 Agent。
/// 预期结果：
///   - 正断言：日志里只有一条含“Codex”和“Cursor”的 prompt，且这条里带着目标文字
///   - 负断言：不会各自单独发一条；目标没有单独成为一条 prompt
#[test]
fn finished_agents_are_batched_and_goal_is_included() {
    let h = start();
    h.service.push(BrainEvent::Goal {
        agent: "Alan".into(),
        text: "今天发布 0.2，之后补测试".into(),
    });
    for agent in ["Codex", "Cursor"] {
        h.service.push(BrainEvent::AgentFinished {
            agent: agent.into(),
            outcome: brain::Outcome::Succeeded,
            excerpt: "完成".into(),
            from_result: true,
        });
    }
    h.wait_log("Cursor 已完成");
    let log = h.log();
    let batched: Vec<_> = log
        .lines()
        .filter(|l| l.contains("Codex 已完成") && l.contains("Cursor 已完成"))
        .collect();
    assert_eq!(batched.len(), 1, "两条完成应合并在同一条 prompt：\n{log}");
    assert!(
        batched[0].contains("今天发布 0.2，之后补测试"),
        "提示里要带目标：{}",
        batched[0]
    );
    let alone = log
        .lines()
        .filter(|l| l.starts_with("prompt:") && l.contains("【目标】"))
        .count();
    assert_eq!(alone, 0, "目标不应单独成为一条 prompt：\n{log}");
    h.service.shutdown();
}

fn start_with(workers: usize) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("fake.log");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_acp.py");
    let backend = Backend {
        kind: BackendKind::ClaudeCode,
        program: "python3".into(),
        args: vec![script.display().to_string()],
    };
    let mut config = BrainConfig::new(
        backend,
        NORMA.into(),
        dir.path().to_path_buf(),
        dir.path().join("state"),
    );
    config.env = vec![("FAKE_LOG".into(), log.display().to_string())];
    config.retry_backoff = Duration::from_millis(50);
    config.router = RouterConfig {
        debounce: Duration::from_millis(50),
        quiet_window: Duration::from_millis(50),
        summary_every: None,
        norma_bin: String::new(),
    };
    config.approval_workers = workers;
    let (service, status) = BrainService::start(config);
    Harness {
        service,
        status,
        log,
        _dir: dir,
    }
}

/// 把两条各要处理 0.6 秒的审批同时交给审批会话，并行时总耗时接近一条，串行时接近两条。
///
/// 数据构造：a5、a6 的命令都含 FAKE_SLEEP（假适配器处理前先睡 0.6 秒）。分别用 1 个和 2 个审批 worker 测。
/// 执行过程：先放一条预热审批让每个会话都启动完，再同时放 a5、a6 并计时到两条都结束。
/// 预期结果：
///   - 正断言：2 个 worker 时两条都在 1.1 秒内结束（0.6 + 余量，小于串行下限 1.2）
///   - 负断言：1 个 worker 时耗时不少于 1.2 秒（两次 0.6 秒串行）
#[test]
fn approval_workers_decide_how_many_approvals_run_in_parallel() {
    for (workers, parallel) in [(2usize, true), (1usize, false)] {
        let h = start_with(workers);
        h.service.push(approval("warm", "cargo check"));
        h.wait_status(|s| matches!(s, BrainStatus::ApprovalTurnEnded { id } if id == "warm"));
        if workers > 1 {
            // 第二个 worker 的会话要等它自己拿到一条活才会启动，先喂一条让它热起来。
            h.service.push(approval("warm2", "cargo check"));
            h.service.push(approval("warm3", "cargo check"));
            h.wait_status(|s| matches!(s, BrainStatus::ApprovalTurnEnded { id } if id == "warm3"));
        }
        let started = Instant::now();
        h.service.push(approval("a5", "cargo test FAKE_SLEEP"));
        h.service.push(approval("a6", "cargo test FAKE_SLEEP"));
        let mut ended = 0;
        while ended < 2 {
            if let BrainStatus::ApprovalTurnEnded { id } =
                h.wait_status(|s| matches!(s, BrainStatus::ApprovalTurnEnded { .. }))
            {
                if id == "a5" || id == "a6" {
                    ended += 1;
                }
            }
        }
        let elapsed = started.elapsed();
        if parallel {
            assert!(
                elapsed < Duration::from_millis(1100),
                "2 个 worker 应并行，实际耗时 {elapsed:?}"
            );
        } else {
            assert!(
                elapsed >= Duration::from_millis(1200),
                "1 个 worker 应串行，实际耗时 {elapsed:?}"
            );
        }
        h.service.shutdown();
    }
}

/// ACP 过程被完整记录：输入、思考、工具调用与结果、计划、未知类型、输出、轮次结束，且按发生顺序。
///
/// 数据构造：用户请求带 FAKE_TRACE，假适配器依次发出思考两段、工具调用、结果、计划、未知更新、回复两段。
/// 预期结果：
///   - 正断言：第一条是 Prompt 且含完整提示；思考合并为“想想看”；回复合并为“先收到”；
///     工具调用、结果、计划、未知类型各一条；最后是 Turn；lane 为 Main，轮次为 1
///   - 负断言：思考与回复没有被拆成多条
#[test]
fn acp_process_is_traced_in_order() {
    let h = start();
    h.service.push(BrainEvent::UserRequest {
        text: "FAKE_TRACE 请回答".into(),
    });
    let mut traces: Vec<AcpTrace> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match h.status.recv_timeout(left) {
            Ok(BrainStatus::Trace(trace)) => {
                let done = trace.item.kind == TraceKind::Turn;
                traces.push(trace);
                if done {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => panic!("等不到 Turn 记录，已收到：{traces:#?}"),
        }
    }
    let main: Vec<&AcpTrace> = traces.iter().filter(|t| t.turn == 1).collect();
    let kinds: Vec<TraceKind> = main.iter().map(|t| t.item.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TraceKind::Prompt,
            TraceKind::Thought,
            TraceKind::ToolCall,
            TraceKind::ToolResult,
            TraceKind::Plan,
            TraceKind::Unknown,
            TraceKind::Reply,
            TraceKind::Turn,
        ],
        "顺序与种类：{main:#?}"
    );
    assert!(main[0].item.text.contains("FAKE_TRACE 请回答"));
    assert!(
        main[0].item.text.contains("你是 Norma 的后台大脑"),
        "首轮提示含角色说明"
    );
    assert_eq!(main[1].item.text, "想想看");
    assert!(main[2].item.text.contains("norma brain agents"));
    assert!(main[3].item.text.contains("agents-ok"));
    assert_eq!(main[6].item.text, "先收到");
    assert!(main[7].item.text.contains("end_turn"));
    assert!(main.iter().all(|t| t.lane == "Main"));
    h.service.shutdown();
}

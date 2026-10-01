//! Foreground fixture for transport/receipt tests; real Runtime writes are tested in headless GPUI separately.
use app::{LiveCommandRequest, LiveTerminalRequest, MobileCommand, ThreadTerminalOperation};
use runtime::TerminalInputLease;
use serde_json::json;
use std::{
    io::Write,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[derive(Default)]
pub struct State {
    owner: Mutex<Option<Arc<TerminalInputLease>>>,
    input_count: AtomicUsize,
    stop: AtomicBool,
    queued: Mutex<Option<LiveCommandRequest>>,
}

pub type PendingRead = (Arc<TerminalInputLease>, crossbeam_channel::Sender<()>);

pub fn serve_reads(
    receiver: async_channel::Receiver<LiveTerminalRequest>,
    state: Arc<State>,
    pending: crossbeam_channel::Sender<PendingRead>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(call) = receiver.recv_blocking() {
            let Some(lease) = call.control.as_ref() else {
                let mut page = super::thread_terminal::page(&call);
                page.input_ready = false;
                let _ = call.reply.send(Ok(page));
                continue;
            };
            let owner = state.owner.lock().expect("fixture输入权").clone();
            if lease.is_revoked()
                || !owner
                    .as_ref()
                    .is_some_and(|owner| Arc::ptr_eq(owner, lease))
            {
                let _ = call.reply.send(Err("terminal_control_expired"));
                continue;
            }
            if call.request.before == Some(77) {
                let (resume, waiting) = crossbeam_channel::bounded(1);
                pending
                    .send((lease.clone(), resume))
                    .expect("报告已在途读取");
                //5s = production foreground4s +1s fixture cleanup margin.
                waiting
                    .recv_timeout(Duration::from_secs(5))
                    .expect("撤权后释放在途fixture");
                assert!(lease.is_revoked(), "权限变更必须同步撤权，不等fixture响应");
                let _ = call.reply.send(Err("terminal_control_expired"));
            } else {
                let mut page = super::thread_terminal::page(&call);
                page.input_ready = call.request.before.is_none();
                let _ = call.reply.send(Ok(page));
            }
        }
    })
}

fn commands(
    receiver: async_channel::Receiver<LiveCommandRequest>,
    state: Arc<State>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !state.stop.load(Ordering::Acquire) {
            let Ok(call) = receiver.try_recv() else {
                //2ms sparse fixture polling is below the20ms receipt polling period.
                std::thread::sleep(Duration::from_millis(2));
                continue;
            };
            if call.request.operation_id == "queued-claim" {
                *state.queued.lock().expect("排队接管") = Some(call);
                continue;
            }
            assert!(call.lease.begin(), "持久命令只进入fixture前台一次");
            let MobileCommand::ThreadTerminal { command } = &call.request.command else {
                panic!("专用终端协议不得派新任务");
            };
            assert_eq!(command.target.task_ref, "test-runtime:7", "不换原任务");
            assert_eq!(
                command.target.key.turn_id, "original-turn",
                "来源必须固定原回合"
            );
            assert_eq!(call.request.route_id, "route-a", "手机不能改路由");
            let mut owner = state.owner.lock().expect("fixture输入权");
            let active = owner.as_ref().filter(|lease| !lease.is_revoked());
            let result = if command.lease.is_revoked() {
                Err("terminal_control_expired")
            } else {
                match &command.operation {
                    ThreadTerminalOperation::Claim { .. } => {
                        if active.is_some_and(|lease| !Arc::ptr_eq(lease, &command.lease)) {
                            command.lease.revoke();
                            Err("terminal_controlled")
                        } else {
                            *owner = Some(command.lease.clone());
                            Ok(json!({"status":"controlled"}))
                        }
                    }
                    ThreadTerminalOperation::Input { input, .. } => {
                        assert!(
                            active.is_some_and(|lease| Arc::ptr_eq(lease, &command.lease)),
                            "只有连接持有者可输入"
                        );
                        assert!(
                            matches!(input, norma::mobile::TerminalInput::Text { text } if text == "draft")
                                || (std::env::var_os("NORMA_TERMINAL_UI_FIXTURE").is_some()
                                    && matches!(input, norma::mobile::TerminalInput::Enter)),
                            "只有显式fixture的draft和UI回车可到前台"
                        );
                        let count = state.input_count.fetch_add(1, Ordering::AcqRel) + 1;
                        Ok(json!({"status":"written","input_count":count}))
                    }
                    ThreadTerminalOperation::Release { .. } => {
                        assert!(
                            active.is_some_and(|lease| Arc::ptr_eq(lease, &command.lease)),
                            "交还不能影响别的持有者"
                        );
                        command.lease.revoke();
                        Ok(json!({"status":"released"}))
                    }
                    ThreadTerminalOperation::Read { .. } => panic!("续期读取不能入持久命令表"),
                }
            };
            let result = result.map(|mut result| {
                result["key"] = json!(command.target.key);
                result["task_ref"] = json!(command.target.task_ref);
                result
            });
            let _ = call.reply.send(result);
        }
    })
}

pub fn finish(
    server: &app::MobileServer,
    receiver: async_channel::Receiver<LiveCommandRequest>,
    state: Arc<State>,
    pending: crossbeam_channel::Receiver<PendingRead>,
    input: &mut impl Write,
    output: &mut impl std::io::BufRead,
) {
    let worker = commands(receiver, state.clone());
    assert_eq!(
        super::line(output)["phase"],
        "terminal-queued-closed",
        "接管尚未到前台时手机断开"
    );
    //5s = foreground4s +1s local scheduling margin; this bounded wait only observes the existing request.
    let until = std::time::Instant::now() + Duration::from_secs(5);
    let queued = loop {
        if let Some(call) = state.queued.lock().expect("排队命令").take() {
            break call;
        }
        assert!(std::time::Instant::now() < until, "排队接管必须实际到达");
        std::thread::sleep(Duration::from_millis(2));
    };
    let MobileCommand::ThreadTerminal { command } = &queued.request.command else {
        panic!("原接管命令");
    };
    assert!(
        command.lease.is_revoked(),
        "关闭连接必须撤销尚未开始的原接管能力"
    );
    queued
        .reply
        .send(Err("terminal_control_expired"))
        .expect("迟到前台拒绝原接管");
    writeln!(input, "queued-rejected").expect("核对失败回执");
    assert_eq!(
        super::line(output)["phase"],
        "terminal-read-pending",
        "必须进入正在处理的控制读取"
    );
    //5s = same4s foreground budget +1s communication scheduling margin.
    let (lease, resume) = pending
        .recv_timeout(Duration::from_secs(5))
        .expect("真实请求已进入前台读取fixture");
    server
        .replace_with_all_projects(
            "86AAD6D7-2A37-4D69-BAF7-03C424336A35",
            1,
            storage::MobilePermission::Read,
        )
        .expect("撤销原手机输入权限");
    assert!(
        lease.is_revoked(),
        "撤权返回前必须关闭租约，不能等待阻塞handler返回"
    );
    resume.send(()).expect("允许旧读取结束");
    writeln!(input, "revoked").expect("通知手机观察断开");
    assert_eq!(
        super::line(output)["phase"],
        "terminal-revoked",
        "新旧通道都必须拒绝只读设备接管"
    );
    server
        .replace_with_all_projects(
            "86AAD6D7-2A37-4D69-BAF7-03C424336A35",
            2,
            storage::MobilePermission::Control,
        )
        .expect("恢复项目控制权限");
    assert!(lease.is_revoked(), "恢复权限不能复活旧租约");
    writeln!(input, "restored").expect("通知手机显式重新接管");
    assert_eq!(
        super::line(output)["phase"],
        "terminal-stop-pending",
        "停服时存在新接管"
    );
    let active = state
        .owner
        .lock()
        .expect("输入权")
        .clone()
        .expect("新输入权存在");
    assert!(!active.is_revoked(), "停服前必须真的持有租约");
    assert_eq!(
        state.input_count.load(Ordering::Acquire),
        if std::env::var("NORMA_NATIVE_TERMINAL_CONTROL_CHECK").as_deref() == Ok("1") {
            2
        } else if std::env::var_os("NORMA_TERMINAL_UI_FIXTURE").is_some() {
            // Node writes once; each UI scenario writes draft + Enter exactly once.
            // Full UI acceptance = standard(2) + AX5/background(2) + Node(1) = 5.
            if std::env::var("NORMA_TERMINAL_UI_SCENARIOS").as_deref() == Ok("2") {
                5
            } else {
                3
            }
        } else {
            1
        },
        "输入次数必须等于Node一次及显式Swift/UI场景，重连不能重放"
    );
    server.shutdown();
    assert!(active.is_revoked(), "停服返回前必须同步撤销活动输入权");
    writeln!(input, "stopped").expect("停服");
    assert_eq!(
        super::line(output)["phase"],
        "terminal-stopped",
        "手机探针正常收口"
    );
    state.stop.store(true, Ordering::Release);
    worker.join().expect("命令fixture收口");
}

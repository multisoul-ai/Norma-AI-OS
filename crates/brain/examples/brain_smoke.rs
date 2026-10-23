//! 用真实的 ACP 适配器验证大脑：审批会话应通过闸门执行 `norma approval decide`。
//!
//! 用法：cargo run -p brain --example brain_smoke -- <适配器程序> <假 norma 脚本> <项目目录>
//! 假 norma 脚本只需把收到的参数追加到某个文件，示例见 docs/research。

use brain::risk::ApprovalSubject;
use brain::{
    ApprovalRequest, Backend, BackendKind, BrainConfig, BrainEvent, BrainService, BrainStatus,
};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let (adapter, norma, project) = match (args.next(), args.next(), args.next()) {
        (Some(a), Some(n), Some(p)) => (a, n, p),
        _ => {
            eprintln!("用法：brain_smoke <适配器> <假 norma> <项目目录>");
            std::process::exit(2);
        }
    };
    let backend = Backend {
        kind: BackendKind::ClaudeCode,
        program: adapter.into(),
        args: vec![],
    };
    let state = std::env::temp_dir().join("brain-smoke-state");
    let mut config = BrainConfig::new(backend, norma.into(), project.clone().into(), state);
    config.prompt_timeout = Duration::from_secs(120);
    let (service, status) = BrainService::start(config);
    service.push(BrainEvent::ApprovalRequested(ApprovalRequest {
        id: "smoke-1".into(),
        agent: "Claude".into(),
        subject: ApprovalSubject {
            tool_name: "Bash".into(),
            command: Some("cargo test -p app".into()),
            file_path: None,
            project_root: project.into(),
        },
    }));
    let started = std::time::Instant::now();
    while let Ok(event) = status.recv_timeout(Duration::from_secs(150)) {
        println!("[{:>5.1}s] {event:?}", started.elapsed().as_secs_f32());
        if matches!(
            event,
            BrainStatus::ApprovalTurnEnded { .. } | BrainStatus::BackendDown { .. }
        ) {
            break;
        }
    }
    service.shutdown();
}

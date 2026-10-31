use super::*;
use crate::{LiveNode, NodeInput};

fn key(key: terminal::Key) -> NodeInput {
    NodeInput::Key(terminal::KeyInput {
        key,
        ctrl: false,
        alt: false,
        shift: false,
    })
}

fn command(cwd: &Path, home: &Path, extra: &str) -> PtyCommand {
    PtyCommand::new("/bin/zsh")
        .args(["-lic", &format!("exec codex -c disable_paste_burst=true -c check_for_update_on_startup=false --dangerously-bypass-approvals-and-sandbox --disable hooks --no-alt-screen{extra}")])
        .cwd(cwd)
        .env("CODEX_HOME", home.to_string_lossy())
        .env("MULTISOUL_AGENT_PROFILE", "codex")
        .env("MULTISOUL_NODE_ID", "7")
        .env(norma::agent_hook::ENV_CONTROL_RUN_ID, "owned-process")
}

/// 复制用户的登录与模型配置，以及一个本仓库的已保存会话，fork 只写入临时目录。
fn isolated_home(cwd: &Path) -> (tempfile::TempDir, String) {
    let user = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".codex");
    let home = tempfile::tempdir().unwrap();
    for file in ["auth.json", "config.toml"] {
        std::fs::copy(user.join(file), home.path().join(file)).unwrap();
    }
    let needle = format!("\"cwd\":\"{}\"", cwd.display());
    let saved = std::fs::read_dir(user.join("sessions/2026/10/28"))
        .unwrap()
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            std::fs::read_to_string(path).is_ok_and(|text| {
                text.lines()
                    .next()
                    .is_some_and(|line| line.contains(&needle))
            })
        })
        .min_by_key(|path| std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX))
        .expect("需要一条本仓库的已保存 Codex 会话");
    let target = home.path().join("sessions/2026/10/28");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::copy(&saved, target.join(saved.file_name().unwrap())).unwrap();
    let name = saved.file_stem().unwrap().to_string_lossy();
    let id = name[name.len() - 36..].to_owned();
    (home, id)
}

fn screen_text(live: &mut TermLive) -> String {
    live.drain();
    match live.view() {
        Some(crate::NodeView::Term { snapshot, .. }) => snapshot
            .rows
            .iter()
            .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// 场景：YOLO 启动的原生 Codex 经私有 relay 运行时，按会话 id 恢复后再 /fork。
/// 构造：真实 CLI、独立 CODEX_HOME（复制登录、配置与一条已保存会话），不发送模型任务。
/// 执行：`resume <id>` 启动，/status 查看权限，再 /fork 当前会话。
/// 预期：恢复与 fork 均无“Permission overrides”拒绝；恢复后仍为免审批+完全访问；
/// 问答宿主可用，fork 只在隔离目录写入新会话。（/resume 选择器需真实 App 验收。）
#[test]
#[ignore = "requires installed Codex 0.166.0 and ~/.codex login; no model requests"]
fn native_codex_real_yolo_resume_and_fork() {
    let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let (home, id) = isolated_home(&cwd);
    let control = Arc::new(LaunchControl::default());
    let resume = format!(" resume {id}");
    let mut live = prepare(1, command(&cwd, home.path(), &resume), 140, 40, control)
        .unwrap()
        .into_live()
        .unwrap();
    let mut host = None;
    let mut screen = String::new();
    let mut stage = 0;
    let mut status = String::new();
    let mut since = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline && stage < 3 {
        screen = screen_text(&mut live);
        if host.is_none() {
            host = live.take_native_question_host();
        }
        if screen.contains("Permission overrides") {
            break;
        }
        let ready = host.is_some()
            && screen.contains("Ask Codex")
            && !screen.contains("Resuming session")
            && since.elapsed() > Duration::from_secs(3);
        let command = |live: &mut TermLive, text: &str| {
            live.input(NodeInput::Text(text.into())).unwrap();
            std::thread::sleep(Duration::from_millis(200));
            live.input(key(terminal::Key::Enter)).unwrap();
        };
        match stage {
            0 if ready => {
                command(&mut live, "/status");
                stage = 1;
            }
            1 if since.elapsed() > Duration::from_secs(2) => {
                status = screen.clone();
                command(&mut live, "/fork");
                stage = 2;
            }
            2 if since.elapsed() > Duration::from_secs(5) => stage = 3,
            _ => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        }
        since = Instant::now();
    }
    let available =
        matches!(&host, Some(crate::live::NativeQuestionHost::Codex(host)) if host.is_available());
    live.kill();
    drop(host);
    eprintln!("status screen:\n{status}\nfinal screen:\n{screen}");
    assert!(
        !screen.contains("Permission overrides"),
        "resume 与 /fork 不能因权限覆盖被拒：{screen}"
    );
    assert_eq!(stage, 3, "必须完成 resume、status 与 fork：{screen}");
    assert!(
        status.contains("Full Access") || status.contains("danger-full-access"),
        "恢复后的会话保持 YOLO：{status}"
    );
    assert!(available, "切换会话后问答宿主仍可用");
    let sessions = walkdir_count(&home.path().join("sessions"));
    assert!(
        sessions >= 2,
        "fork 应在隔离 CODEX_HOME 写入新会话，实际 {sessions}"
    );
}

fn walkdir_count(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                walkdir_count(&path)
            } else {
                usize::from(path.extension().is_some_and(|ext| ext == "jsonl"))
            }
        })
        .sum()
}

use super::*;
use crate::protocol::AgentProfile;
use serde_json::json;
use std::io::Write;

fn write(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let mut file = File::create(&path).unwrap();
    for line in lines {
        writeln!(file, "{line}").unwrap();
    }
    path
}

fn event(profile: AgentProfile, name: &str, raw: Value) -> AgentHookEvent {
    AgentHookEvent {
        control_run_id: None,
        node_id: 1,
        profile,
        provider_event: name.into(),
        session_id: Some("s1".into()),
        turn_id: None,
        tool_name: None,
        message: None,
        raw_json: raw,
    }
}

/// Claude 的 transcript 里 ai-title 会重复写入，取最后一条；空标题和损坏行被跳过。
/// 数据构造：旧标题、用户消息、新标题、空标题、半截 JSON。
/// 预期结果：
///   - 正断言：返回最后一条非空标题，并折叠多余空白
///   - 负断言：没有 ai-title 的文件返回 None；文件不存在返回 None
#[test]
fn claude_takes_last_non_empty_ai_title() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "t.jsonl",
        &[
            r#"{"type":"ai-title","aiTitle":"旧标题"}"#,
            r#"{"type":"user","message":"hi"}"#,
            r#"{"type":"ai-title","aiTitle":"  修复   幕僚  权限 "}"#,
            r#"{"type":"ai-title","aiTitle":"  "}"#,
            r#"{"type":"ai-title","aiTi"#,
        ],
    );
    assert_eq!(claude_ai_title(&path).as_deref(), Some("修复 幕僚 权限"));
    let plain = write(dir.path(), "p.jsonl", &[r#"{"type":"user"}"#]);
    assert_eq!(claude_ai_title(&plain), None);
    assert_eq!(claude_ai_title(&dir.path().join("missing")), None);
}

/// Codex 的索引文件有多个会话，且同一会话改名后追加新行。
/// 预期结果：只取匹配 id 的最后一条 thread_name；其他会话和空名不影响。
#[test]
fn codex_takes_last_thread_name_for_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "session_index.jsonl",
        &[
            r#"{"id":"s1","thread_name":"初稿"}"#,
            r#"{"id":"s2","thread_name":"别的会话"}"#,
            r#"{"id":"s1","thread_name":"撰写自我介绍"}"#,
            r#"{"id":"s1","thread_name":""}"#,
        ],
    );
    assert_eq!(
        codex_thread_name(&path, "s1").as_deref(),
        Some("撰写自我介绍")
    );
    assert_eq!(codex_thread_name(&path, "none"), None);
}

/// 读取尾部时，起点落在半行中间的残片不能被当作标题。
/// 数据构造：把超过读取上限的填充行放在唯一标题之前。
/// 预期结果：标题行位于尾部范围内时仍被找到。
#[test]
fn title_is_found_when_file_is_larger_than_the_tail_window() {
    let dir = tempfile::tempdir().unwrap();
    let filler = "x".repeat(1024);
    let mut lines: Vec<String> = (0..3000)
        .map(|_| format!(r#"{{"type":"user","m":"{filler}"}}"#))
        .collect();
    lines.push(r#"{"type":"ai-title","aiTitle":"尾部标题"}"#.into());
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let path = write(dir.path(), "big.jsonl", &refs);
    assert!(std::fs::metadata(&path).unwrap().len() > TAIL_BYTES);
    assert_eq!(claude_ai_title(&path).as_deref(), Some("尾部标题"));
}

/// 只有提交提示词和一轮结束才刷新；其他 CLI 或缺少来源时回退（返回 None）。
/// 预期结果：事件过滤正确；Claude 事件按 transcript_path 取标题；无路径、OpenCode 返回 None。
#[test]
fn refresh_events_and_per_profile_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "t.jsonl",
        &[r#"{"type":"ai-title","aiTitle":"标题"}"#],
    );
    let claude = event(
        AgentProfile::Claude,
        "Stop",
        json!({"transcript_path": path.to_string_lossy()}),
    );
    assert!(should_refresh(&claude));
    assert!(should_refresh(&event(
        AgentProfile::Codex,
        "SessionStart",
        json!({})
    )));
    assert!(should_refresh(&event(
        AgentProfile::Codex,
        "PostToolUse",
        json!({})
    )));
    assert!(!should_refresh(&event(
        AgentProfile::Claude,
        "PreToolUse",
        json!({})
    )));
    assert_eq!(session_title(&claude).as_deref(), Some("标题"));
    assert_eq!(
        session_title(&event(AgentProfile::Claude, "Stop", json!({}))),
        None
    );
    let opencode = event(
        AgentProfile::OpenCode,
        "Stop",
        json!({"transcript_path": path.to_string_lossy()}),
    );
    assert_eq!(session_title(&opencode), None);
}

/// 场景：打开项目恢复 Claude 会话，只知道会话 id，不知道在哪个项目目录。
/// 数据构造：projects 下有两个项目目录，只有 proj-b 里有 s1.jsonl（含 ai-title「恢复的标题」）。
/// 预期：能跨项目目录找到并读到标题；找不到的 id 返回 None。
#[test]
fn claude_title_found_by_session_id_across_project_dirs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("proj-a")).unwrap();
    std::fs::create_dir_all(dir.path().join("proj-b")).unwrap();
    write(
        &dir.path().join("proj-b"),
        "s1.jsonl",
        &[r#"{"type":"ai-title","aiTitle":"恢复的标题"}"#],
    );
    assert_eq!(
        claude_title_in(dir.path(), "s1").as_deref(),
        Some("恢复的标题")
    );
    assert_eq!(claude_title_in(dir.path(), "missing"), None);
}

/// 场景：hook 传来的 transcript_path 不可信，session id 也可能含路径分隔符。
/// 预期：目录、含 `..` 的 id 都返回 None，不会读到项目目录之外的文件，也不会卡在非普通文件上。
#[test]
fn untrusted_paths_and_ids_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("proj")).unwrap();
    write(
        dir.path(),
        "secret.jsonl",
        &[r#"{"type":"ai-title","aiTitle":"越界"}"#],
    );
    assert_eq!(claude_title_in(&dir.path().join("proj"), "../secret"), None);
    assert_eq!(claude_ai_title(dir.path()), None, "目录不是 transcript");
}

#![cfg(unix)]
use norma::claude::{read_transcript, transcript_end, TranscriptCursor};
use serde_json::{json, Value};
use std::{fs, io::Write, path::Path};

fn message(id: &str, session: &str, cwd: &Path, role: &str, content: Value) -> Value {
    json!({"type":role,"uuid":id,"parentUuid":null,"sessionId":session,"cwd":cwd,
        "message":{"role":role,"content":content}})
}

/// 完整对话分页保留长文本和工具块，写到一半的尾行不能丢失，其他项目/会话不能被读出。
///
/// 数据构造：5000 个中文字（15000 UTF-8 字节，大于旧 1200 字符摘要）；工具块 1 个；半行等待后续追加。
/// 执行过程：读第一页 1 条 → 第二页工具内容 → 半行返回相同 cursor → 补齐尾行读取 → 伪造会话/路径/偏移读取。
/// 预期结果：完整正文和工具 input 保留；半行不重复或丢字；跨会话、跨项目及非行边界 cursor 均拒绝。
#[test]
fn full_transcript_pages_are_scoped_and_resume_partial_lines() {
    let dir = tempfile::tempdir().expect("测试目录");
    let root = dir.path().join("project");
    fs::create_dir(&root).expect("项目目录");
    let projects = dir.path().join("claude-projects");
    fs::create_dir(&projects).expect("受控会话目录");
    let file = projects.join("session-a.jsonl");
    let text = "长".repeat(5000);
    let tool =
        json!([{"type":"tool_use","id":"tool-1","name":"Bash","input":{"command":"echo hello"}}]);
    let first = message("u1", "session-a", &root, "user", json!(text));
    let second = message("a1", "session-a", &root, "assistant", tool.clone());
    let third = message(
        "a2",
        "session-a",
        &root,
        "assistant",
        json!([{"type":"text","text":"done"}]),
    );
    let encoded = third.to_string();
    let split = encoded.len() / 2;
    fs::write(
        &file,
        format!("{}\n{}\n{}", first, second, &encoded[..split]),
    )
    .expect("应写入两完整行和半行");
    let page = read_transcript(&file, &projects, "session-a", &root, None, 1).expect("第一页");
    assert_eq!(page.messages.len(), 1, "每页必须遵守 limit");
    assert_eq!(page.messages[0].content, json!(text), "不得把正文截成摘要");
    assert_eq!(page.messages[0].id, "u1", "消息必须有稳定 Provider 身份");
    assert!(page.has_more, "后面还有完整数据");
    let page2 = read_transcript(&file, &projects, "session-a", &root, Some(page.cursor), 20)
        .expect("第二页");
    assert_eq!(page2.messages.len(), 1, "半行不得冒充完整消息");
    assert_eq!(page2.messages[0].content, tool, "工具 input 必须完整保留");
    assert!(page2.pending_tail, "必须标明尚未写完的尾行");
    let waiting = read_transcript(
        &file,
        &projects,
        "session-a",
        &root,
        Some(page2.cursor.clone()),
        20,
    )
    .expect("轮询半行");
    assert!(waiting.messages.is_empty(), "半行不能重复显示上一条");
    assert_eq!(waiting.cursor, page2.cursor, "未完整的行不得推进 cursor");
    let mut append = fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .expect("追加文件");
    writeln!(append, "{}", &encoded[split..]).expect("补齐尾行");
    let completed = read_transcript(
        &file,
        &projects,
        "session-a",
        &root,
        Some(waiting.cursor.clone()),
        20,
    )
    .expect("应增量读取");
    assert_eq!(completed.messages[0].id, "a2", "必须只返回补齐的新消息");
    assert!(!completed.pending_tail, "完整尾行不再等待");
    assert!(!completed.has_more, "已读到当前 EOF");
    assert!(
        read_transcript(&file, &projects, "session-b", &root, None, 20).is_err(),
        "不同会话不得访问同一文件"
    );
    let outside = dir.path().join("session-a.jsonl");
    fs::copy(&file, &outside).expect("构造目录外文件");
    assert!(
        read_transcript(&outside, &projects, "session-a", &root, None, 20).is_err(),
        "不能读取会话根目录之外的路径"
    );
    let other = dir.path().join("other");
    fs::create_dir(&other).expect("另一个项目");
    assert!(
        read_transcript(&file, &projects, "session-a", &other, None, 20).is_err(),
        "相同 session 文件内的 cwd 也必须在授权项目"
    );
    let mut value = serde_json::to_value(&completed.cursor).expect("游标JSON");
    value["offset"] = json!(1);
    let wrong: TranscriptCursor = serde_json::from_value(value).expect("构造无效游标");
    assert!(
        read_transcript(&file, &projects, "session-a", &root, Some(wrong), 20).is_err(),
        "游标必须落在完整行边界"
    );
    fs::write(&file, "{}\n").expect("模拟文件被截断");
    assert!(
        read_transcript(
            &file,
            &projects,
            "session-a",
            &root,
            Some(completed.cursor),
            20
        )
        .is_err(),
        "截断后不能继续使用旧游标"
    );
}

/// 中断证据只能来自发送前 EOF 之后，用户输入的同名字面文本不属于 Provider 中断。
///
/// 数据构造：旧中断 1 条，新 typed 输入 1 条，Provider 中断 1 条；新中断先写一半再补齐。
/// 执行过程：捕获 EOF → 追加用户仿冒标记与半条新记录 → 读取 → 补齐 → 增量读取 → 换文件。
/// 预期结果：旧记录和 typed 标记不能证明中断；半行不推进，补齐后才有准确 promptId 的中断；换文件拒绝。
#[test]
fn interrupt_evidence_starts_at_complete_eof_and_excludes_typed_markers() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("session-a.jsonl");
    let mut marker = message(
        "old",
        "session-a",
        dir.path(),
        "user",
        json!([{"type":"text","text":"[Request interrupted by user for tool use]"}]),
    );
    marker["promptId"] = json!("turn-a");
    fs::write(&path, format!("{marker}\n")).expect("旧记录");
    let cursor = transcript_end(&path, dir.path(), "session-a", dir.path()).expect("捕获末尾");
    let mut typed = marker.clone();
    typed["uuid"] = json!("typed");
    typed["origin"] = json!({"kind":"human"});
    typed["promptSource"] = json!("typed");
    marker["uuid"] = json!("new");
    let encoded = marker.to_string();
    let split = encoded.len() / 2; // 对称分为两次写入，制造真实 JSONL 半行。
    let mut append = fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("追加");
    write!(append, "{typed}\n{}", &encoded[..split]).expect("用户输入和半行");
    assert!(
        transcript_end(&path, dir.path(), "session-a", dir.path()).is_err(),
        "未完成的旧记录不能成为发送前边界；不得从半行开始解析"
    );
    let page = read_transcript(&path, dir.path(), "session-a", dir.path(), Some(cursor), 20)
        .expect("新页");
    assert_eq!(
        page.messages.len(),
        1,
        "只能读到 typed，不能重复旧中断或解析半行"
    );
    assert!(!page.messages[0].interrupted, "用户输入字面标记不等于中断");
    assert!(page.pending_tail, "必须等待半条新记录");
    writeln!(append, "{}", &encoded[split..]).expect("补齐");
    let page = read_transcript(
        &path,
        dir.path(),
        "session-a",
        dir.path(),
        Some(page.cursor),
        20,
    )
    .expect("补齐页");
    assert_eq!(page.messages[0].id, "new", "只能使用新追加证据");
    assert_eq!(
        page.messages[0].prompt_id.as_deref(),
        Some("turn-a"),
        "证据必须携带精确回合"
    );
    assert!(
        page.messages[0].interrupted,
        "Provider 单独中断记录应可识别"
    );
    assert!(!page.pending_tail, "完整记录不再等待");
    fs::rename(&path, dir.path().join("old.jsonl")).expect("替换 inode");
    fs::write(&path, format!("{marker}\n{marker}\n{marker}\n"))
        .expect("新文件更长也不能复用旧游标");
    assert!(
        read_transcript(
            &path,
            dir.path(),
            "session-a",
            dir.path(),
            Some(page.cursor),
            20
        )
        .is_err(),
        "替换后的文件不能提供旧运行的中断证据"
    );
}

use norma::cursor::{read_transcript, TranscriptCursor};
use norma::mobile::ConversationPage;
use std::fs;

fn fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let dir = tempfile::tempdir().expect("隔离目录");
    let workspace = dir.path().join("Work Space");
    fs::create_dir(&workspace).expect("创建workspace");
    let workspace = workspace.canonicalize().expect("规范化workspace");
    let projects = dir.path().join(".cursor/projects");
    let encoded = workspace
        .strip_prefix("/")
        .expect("临时目录是绝对路径")
        .to_string_lossy()
        .replace('/', "-");
    let transcript = projects
        .join(encoded)
        .join("agent-transcripts/session-a/session-a.jsonl");
    fs::create_dir_all(transcript.parent().expect("transcript目录")).expect("创建transcript目录");
    (dir, workspace, projects, transcript)
}

/// Cursor JSONL按完整记录分页，文本和工具块都可在iOS中恢复且游标不会重复消息。
///
/// 数据构造：同一session含user文本、assistant文本+tool_use，以及非消息turn_ended；每页限制1条消息。
/// 执行过程：读取首页 → 序列化/反序列化游标 → 读取续页 → 投影成移动端ConversationPage。
/// 预期结果：首页只有user，续页只有assistant；工具详情不丢失；稳定ID由原始字节位置生成且方向为newer。
#[test]
fn cursor_transcript_pages_losslessly_from_bound_project() {
    let (_dir, workspace, projects, transcript) = fixture();
    fs::write(
        &transcript,
        concat!(
            "{\"role\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"实现 Cursor 卡片\"}]}}\n",
            "{\"role\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"已经完成\"},{\"type\":\"tool_use\",\"name\":\"Read\",\"input\":{\"path\":\"README.md\"}}]}}\n",
            "{\"type\":\"turn_ended\",\"status\":\"success\"}\n"
        ),
    )
    .expect("写入Cursor transcript");

    let first = read_transcript(&transcript, &projects, "session-a", &workspace, None, 1)
        .expect("读取首页");
    assert_eq!(first.messages.len(), 1, "首页限制1条消息");
    assert_eq!(first.messages[0].role, "user", "首条必须是用户消息");
    assert_eq!(first.messages[0].id, "cursor:0", "首条稳定ID来自字节位置");
    assert!(first.has_more, "后续还有assistant和事件记录");
    assert!(!first.pending_tail, "完整换行记录不能标记为半截");
    let cursor: TranscriptCursor =
        serde_json::from_str(&serde_json::to_string(&first.cursor).expect("编码游标"))
            .expect("解码游标");

    let second = read_transcript(
        &transcript,
        &projects,
        "session-a",
        &workspace,
        Some(cursor),
        1,
    )
    .expect("读取续页");
    let page = ConversationPage::from_cursor(second).expect("生成移动端历史");
    assert_eq!(page.items.len(), 1, "续页只投影assistant消息");
    assert_eq!(page.items[0].role, "assistant", "Provider回复身份准确");
    assert_eq!(page.items[0].text, "已经完成", "文本块不能丢失");
    assert!(
        page.items[0]
            .details
            .as_deref()
            .is_some_and(|value| value.contains("tool_use") && value.contains("README.md")),
        "工具块必须作为可展开详情保留"
    );
    assert_eq!(page.direction, "newer", "Cursor JSONL续页沿时间向后读取");
    assert!(page.has_more, "非消息尾记录仍需由下一页推进游标");
}

/// transcript必须精确属于workspace编码目录和session目录；不完整尾行留给下一次读取。
///
/// 数据构造：准确路径内一条完整user记录和一条未闭合assistant记录；另建跨项目同名文件。
/// 执行过程：读取准确文件 → 尝试读取跨项目文件 → 补全尾行后用原游标继续。
/// 预期结果：首读只返回完整记录且pending_tail；跨项目拒绝；补全后恰好返回assistant且不重复user。
#[test]
fn cursor_transcript_rejects_cross_project_and_retries_partial_tail() {
    let (dir, workspace, projects, transcript) = fixture();
    let first_line = "{\"role\":\"user\",\"message\":{\"content\":\"hello\"}}\n";
    let partial = "{\"role\":\"assistant\",\"message\":{\"content\":\"done";
    fs::write(&transcript, format!("{first_line}{partial}")).expect("写入半截记录");
    let first = read_transcript(&transcript, &projects, "session-a", &workspace, None, 20)
        .expect("完整部分可读");
    assert_eq!(first.messages.len(), 1, "半截assistant不得泄漏或丢弃");
    assert!(first.pending_tail, "应明确通知调用端等待尾行完成");
    assert!(!first.has_more, "半截尾行不能被当作可翻页的完整历史");

    let foreign_workspace = dir.path().join("Foreign");
    fs::create_dir(&foreign_workspace).expect("创建外部workspace");
    let foreign_dir = projects.join("Foreign/agent-transcripts/session-a");
    fs::create_dir_all(&foreign_dir).expect("创建外部transcript目录");
    let foreign = foreign_dir.join("session-a.jsonl");
    fs::write(&foreign, first_line).expect("写入外部记录");
    assert!(
        read_transcript(&foreign, &projects, "session-a", &workspace, None, 20).is_err(),
        "远端请求不能借同名session读取另一个项目"
    );

    fs::write(&transcript, format!("{first_line}{partial}\"}}}}\n")).expect("补全尾行");
    let second = read_transcript(
        &transcript,
        &projects,
        "session-a",
        &workspace,
        Some(first.cursor),
        20,
    )
    .expect("从原边界重读尾行");
    assert_eq!(second.messages.len(), 1, "只应返回新补全的assistant");
    assert_eq!(second.messages[0].role, "assistant", "补全记录角色准确");
    assert_eq!(second.messages[0].content, "done", "补全正文必须完整");
    assert!(!second.pending_tail, "补全后不再等待");
}

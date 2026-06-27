use super::*;

fn history_entry(transcript: &str, status: &str) -> VoiceHistoryEntry {
    VoiceHistoryEntry {
        transcript: transcript.to_string(),
        raw_transcript: transcript.to_string(),
        refined_transcript: None,
        planner_json: r#"{"commands":[]}"#.to_string(),
        confidence: 0.82,
        risk: "low".to_string(),
        status: status.to_string(),
        provider: Some("ollama".to_string()),
        model: Some("llama3.1".to_string()),
        target_node_id: Some(7),
        error: None,
    }
}

/// VH-1: VoiceHistoryStore 保存最小可审计记录，不需要音频或完整 prompt。
///
/// 数据构造（含关键数值的推导过程）：
///   transcript = "创建 Codex"
///   confidence = 0.82
///   target_node_id = 7
///   planner_json = {"commands":[]}，只保存 planner 输出草案，不保存 prompt
///
/// 执行过程（逐步说明系统如何处理）：
///   1. save_entry(entry) → 插入 voice_history
///   2. load_recent(5) → 按 id DESC 读取最近记录
///   3. 检查字段完整性
///
/// 预期结果：
///   - 正断言：读回记录保留 transcript/planner_json/status/provider/model
///   - 正断言：created_at_unix 大于 0
///   - 负断言：planner_json 不应包含 prompt 字段
#[test]
fn test_voice_history_round_trips_minimal_audit_record() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = VoiceHistoryStore::new(temp.path().join("project.sqlite"));
    let entry = history_entry("创建 Codex", "executed");

    let id = store
        .save_entry(&entry)
        .expect("voice history entry 应写入 SQLite");
    let records = store
        .load_recent(5)
        .expect("voice history 应可从 SQLite 读回");

    assert_eq!(records.len(), 1, "应读回 1 条 voice history 记录");
    assert_eq!(records[0].id, id, "读回 id 应等于插入返回 id");
    assert_eq!(
        records[0].transcript, "创建 Codex",
        "transcript 应完整保存用于用户可见排障"
    );
    assert_eq!(
        records[0].planner_json, r#"{"commands":[]}"#,
        "planner 输出草案应保存为 JSON 字符串"
    );
    assert_eq!(records[0].status, "executed", "状态应保存为 executed");
    assert_eq!(
        records[0].provider.as_deref(),
        Some("ollama"),
        "provider 应记录为 ollama"
    );
    assert_eq!(
        records[0].model.as_deref(),
        Some("llama3.1"),
        "model 应记录用于复盘具体本地模型"
    );
    assert!(
        records[0].created_at_unix > 0,
        "created_at_unix 应写入当前 unix 时间"
    );
    assert!(
        !records[0].planner_json.contains("prompt"),
        "最小审计记录不应保存完整 prompt"
    );
}

/// VH-2: load_recent 应按新到旧排序并遵守 limit。
///
/// 数据构造（含关键数值的推导过程）：
///   插入 3 条记录：first/second/third
///   limit = 2，因此最多返回 2 条
///   id 自增，third 的 id 最大，应排在最前
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 顺序保存 first、second、third
///   2. load_recent(2)
///   3. 检查返回顺序与数量
///
/// 预期结果：
///   - 正断言：返回 2 条
///   - 正断言：顺序为 third、second
///   - 负断言：first 不应出现在 limit=2 的结果中
#[test]
fn test_voice_history_recent_order_and_limit() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = VoiceHistoryStore::new(temp.path().join("project.sqlite"));

    store
        .save_entry(&history_entry("first", "executed"))
        .expect("first entry 应写入");
    store
        .save_entry(&history_entry("second", "transcript_only"))
        .expect("second entry 应写入");
    store
        .save_entry(&history_entry("third", "provider_error"))
        .expect("third entry 应写入");

    let records = store.load_recent(2).expect("recent voice history 应可读取");

    assert_eq!(records.len(), 2, "limit=2 时最多应返回 2 条记录");
    assert_eq!(
        records[0].transcript, "third",
        "最新插入的 third 应排在第一位"
    );
    assert_eq!(
        records[1].transcript, "second",
        "第二新的 second 应排在第二位"
    );
    assert!(
        !records.iter().any(|record| record.transcript == "first"),
        "limit=2 时最旧的 first 不应出现在结果中"
    );
}

/// VH-3: raw_transcript 和 refined_transcript 应分别保存并可读回。
///
/// 数据构造（含关键数值的推导过程）：
///   raw_transcript = "再哪里"（Apple/Sherpa Speech 原始输出）
///   refined_transcript = Some("在哪里")（LLM 修正后）
///   transcript = "在哪里"（用于规划）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 entry，raw 与 refined 不同
///   2. save_entry(entry) → 写入 voice_history
///   3. load_recent(1) → 读取最近 1 条
///   4. 检查两列独立存储
///
/// 预期结果：
///   - 正断言：raw_transcript = "再哪里"
///   - 正断言：refined_transcript = Some("在哪里")
///   - 负断言：raw_transcript 不应等于 refined_transcript（它们不同）
#[test]
fn test_voice_history_stores_raw_and_refined_transcripts() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = VoiceHistoryStore::new(temp.path().join("project.sqlite"));
    let entry = VoiceHistoryEntry {
        transcript: "在哪里".to_string(),
        raw_transcript: "再哪里".to_string(),
        refined_transcript: Some("在哪里".to_string()),
        planner_json: r#"{"commands":[]}"#.to_string(),
        confidence: 0.9,
        risk: "low".to_string(),
        status: "executed".to_string(),
        provider: Some("ollama".to_string()),
        model: Some("qwen2.5:1.5b".to_string()),
        target_node_id: None,
        error: None,
    };

    store.save_entry(&entry).expect("entry 应写入 SQLite");
    let records = store.load_recent(1).expect("应可从 SQLite 读回");

    assert_eq!(records.len(), 1, "应读回 1 条记录");
    assert_eq!(
        records[0].raw_transcript, "再哪里",
        "raw_transcript 应保存 Speech 原始输出"
    );
    assert_eq!(
        records[0].refined_transcript,
        Some("在哪里".to_string()),
        "refined_transcript 应保存 LLM 修正后内容"
    );
    assert_ne!(
        records[0].raw_transcript,
        records[0].refined_transcript.as_deref().unwrap_or(""),
        "同音字修正场景下 raw 与 refined 不应相同"
    );
}

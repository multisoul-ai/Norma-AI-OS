//! G0 R0 离线重放：只执行纯函数，不执行工具副作用或伪造模型结果。
use agent::{AgentRosterEntry, AgentTaskState, AgentTrackingMode};
use norma::protocol::AgentProfile;
use serde_json::{json, Value};

fn roster_fixture() -> Vec<AgentRosterEntry> {
    let data: Value =
        serde_json::from_str(include_str!("../../../test/voice-eval/roster.json")).unwrap();
    data["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| AgentRosterEntry {
            definition_id: a["definition_id"].as_i64().unwrap(),
            name: a["name"].as_str().unwrap().to_owned(),
            speech_alias: a["speech_alias"].as_str().map(str::to_owned),
            profile: AgentProfile::Codex,
            enabled: a["enabled"].as_bool().unwrap(),
            active_node_id: a["active_node_id"].as_u64(),
            tracking: AgentTrackingMode::Managed,
            busy: false,
            task_state: AgentTaskState::Ready,
        })
        .collect()
}

fn replay(text: &str, roster: &[AgentRosterEntry]) -> Value {
    let start = std::time::Instant::now();
    let plan = agent::plan_direct_address(text, roster);
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    match plan {
        agent::DirectPlan::Skip => json!({"path":"model", "targets":null, "action":null,
            "t_route_ms":null, "t_parse_ms":elapsed}),
        agent::DirectPlan::Say(_) => json!({"path":"clarify", "targets":[], "action":"clarify",
            "t_route_ms":elapsed, "t_parse_ms":elapsed}),
        agent::DirectPlan::Calls(calls) => {
            let targets: Vec<_> = calls.iter().map(|c| c.arguments["agent"].clone()).collect();
            let action = if calls.iter().all(|c| c.tool == "reply_agent") {
                "reply"
            } else {
                "assign"
            };
            json!({"path":"direct", "targets":targets, "action":action,
                "t_route_ms":elapsed, "t_parse_ms":elapsed})
        }
    }
}

/// 场景：R0 句首 Daisy 可确定派发，句中点名仍需模型。
/// 构造生产语义的活跃名单；执行纯函数重放；预期 reply 目标为 Daisy，
/// Skip 的最终目标与模型时延为 null，不能误报空目标成功或零毫秒。
#[test]
fn r0_replay_preserves_unknown_model_outcomes() {
    let roster = roster_fixture();
    let direct = replay("Daisy，请运行 cargo test", &roster);
    assert_eq!(
        direct["targets"],
        json!(["Daisy"]),
        "句首点名应走生产解析器"
    );
    assert_eq!(direct["action"], "reply", "活跃卡片默认为会话回复");
    let model = replay("让 Daisy 去修登录 bug", &roster);
    assert_eq!(model["path"], "model", "R0 句中点名应记录模型兜底");
    assert!(
        model["targets"].is_null(),
        "没真实调用模型就不能编造最终目标"
    );
    assert!(model["t_route_ms"].is_null(), "纯函数 Skip 不是模型总时延");
}

/// 场景：把生产 ASR 缓存重放到冻结的 R0 名单，不接触 Storage 或工具执行器。
/// 数据：VOICE_EVAL_ASR JSONL 和公共 roster；执行逐条解析/计时并保存目标。
/// 预期：输出与 ASR 的 id/repeat 一一对应，模型兜底保留未知，既有结果禁止覆盖。
#[test]
#[ignore = "需要已完成的本机 ASR 结果"]
fn replay_production_r0() {
    use sha2::{Digest, Sha256};
    use std::{fs, io::Write, path::PathBuf, process::Command};
    let source = PathBuf::from(std::env::var_os("VOICE_EVAL_ASR").expect("设置 VOICE_EVAL_ASR"));
    let destination =
        PathBuf::from(std::env::var_os("VOICE_EVAL_OUTPUT").expect("设置 VOICE_EVAL_OUTPUT"));
    let bytes = fs::read(&source).expect("读取生产 ASR 结果");
    let rows: Vec<Value> = String::from_utf8(bytes.clone())
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(!rows.is_empty(), "不接受空 ASR 结果");
    fs::create_dir_all(&destination).unwrap();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    // 只允许明确标注为 synthetic-dev/regression 的转写写回仓库。
    let public = rows
        .iter()
        .all(|r| matches!(r["source"].as_str(), Some("synthetic-dev" | "regression")));
    assert!(
        public || !destination.canonicalize().unwrap().starts_with(repo),
        "真人/来源不明的转写只能输出仓库外"
    );
    let roster = roster_fixture();
    let roster_bytes = include_bytes!("../../../test/voice-eval/roster.json");
    let config = json!({"asr":"A0", "route":"R0", "memory":"L0", "roster_sha256":format!("{:x}", Sha256::digest(roster_bytes)),
        "scope":"plan_direct_address only; excludes memory_preamble, model and adapter side effects"});
    let config_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&config).unwrap()));
    let path = destination.join("route-A0-R0-L0.jsonl");
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("禁止覆盖已有结果");
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        assert_eq!(row["group"], "A0", "G0 只接受 A0 生产输出");
        assert!(
            seen.insert((
                row["id"].clone().to_string(),
                row["repeat"].clone().to_string()
            )),
            "同一样本/重复次数不得出现两次"
        );
        let mut decision = replay(row["text"].as_str().unwrap(), &roster);
        decision["id"] = row["id"].clone();
        decision["repeat"] = row["repeat"].clone();
        decision["config_sha256"] = config_hash.clone().into();
        serde_json::to_writer(&mut output, &decision).unwrap();
        writeln!(output).unwrap();
    }
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(commit.status.success(), "必须记录代码版本");
    let manifest = json!({"commit":String::from_utf8(commit.stdout).unwrap().trim(),
        "configuration":config, "config_sha256":config_hash,
        "asr_sha256":format!("{:x}", Sha256::digest(&bytes)),
        "parser_sha256":format!("{:x}", Sha256::digest(include_bytes!("../src/direct_address.rs"))),
        "planner_sha256":format!("{:x}", Sha256::digest(include_bytes!("../src/direct_address_turn.rs"))),
        "runner_sha256":format!("{:x}", Sha256::digest(include_bytes!("voice_eval_route.rs"))),
        "observations":seen.len(), "model_calls":0});
    fs::write(
        destination.join("route-manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

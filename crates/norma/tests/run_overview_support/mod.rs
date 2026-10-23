//! Shared fixtures for the run-overview integration tests.

use norma::mobile::{RunOverviewFile, RunOverviewProjectPresence, WorkspaceActivityFacts};
use serde_json::json;

pub fn presence() -> Vec<RunOverviewProjectPresence> {
    vec![
        RunOverviewProjectPresence {
            area_id: "area-current".into(),
            loaded: true,
            updated_at_unix_ms: None,
        },
        RunOverviewProjectPresence {
            area_id: "area-other".into(),
            loaded: false,
            updated_at_unix_ms: Some(280_000),
        },
    ]
}

pub fn files() -> Vec<RunOverviewFile> {
    vec![
        RunOverviewFile {
            area_id: "area-current".into(),
            node_id: 1,
            agent_hint: "Codex".into(),
            project_hint: String::new(),
            path: "docs/prototypes/norma-assistant/index.html".into(),
            turn_id: "turn-review".into(),
            fingerprint: "fp-index".into(),
        },
        RunOverviewFile {
            area_id: "area-current".into(),
            node_id: 1,
            agent_hint: "Codex".into(),
            project_hint: String::new(),
            path: "src/left.css".into(),
            turn_id: "turn-file".into(),
            fingerprint: "fp-css".into(),
        },
    ]
}

pub fn facts() -> WorkspaceActivityFacts {
    serde_json::from_value(facts_json()).expect("概览事实")
}

pub fn facts_json() -> serde_json::Value {
    json!({
        "workspace": {
            "id": "space-1",
            "name": "Norma OS",
            "revision": 1,
            "projects": [
                {
                    "area_id": "area-current",
                    "project_id": "norma",
                    "name": "Norma-AI-OS",
                    "root": "/local",
                    "definitions": [],
                    "agents": [
                        agent(1, "thinking", "Bella", "调整原型布局与透明材质", true),
                        agent(2, "ready", "Bruno", "检查抽屉响应式布局", false),
                        agent(4, "failed", "Dee", "失败的任务", false)
                    ]
                },
                {
                    "area_id": "area-other",
                    "project_id": "draft",
                    "name": "设计草稿",
                    "root": "/other",
                    "definitions": [],
                    "agents": [agent(3, "waiting_approval", "Cara", "旧方案", false)]
                }
            ]
        },
        "observed": true,
        "approvals": [{
            "area_id": "area-current",
            "approval": {
                "target": {
                    "node_id": 1,
                    "profile": "Codex",
                    "run_id": "run-1",
                    "session_id": "session-1",
                    "turn_id": "turn-1",
                    "request_id": "req-1"
                },
                "card_title": "Bella",
                "tool": "Bash",
                "details": {},
                "decisions": ["allow", "deny"]
            }
        }],
        "reviews": [{
            "area_id": "area-current",
            "node_id": 1,
            "profile": "Codex",
            "run_id": "run-review",
            "session_id": "session-review",
            "turn_id": "turn-review",
            "fingerprint": "fp-review",
            "completed": true,
            "available": true,
            "conflicted": false,
            "accepted": false,
            "file_count": 1
        }]
    })
}

pub fn agent(id: u64, state: &str, name: &str, task: &str, progress: bool) -> serde_json::Value {
    let mut card = json!({
        "node_id": id,
        "profile": "Codex",
        "title": name,
        "mobile_control": true,
        "state": state,
        "run_id": "run-1",
        "session_id": "session-1",
        "turn_id": "turn-1",
        "task_title": task,
        "task_timing": {"started_at_unix_ms": 1_700_000_000_000_u64, "completed_elapsed_ms": null}
    });
    if progress {
        card["task_progress"] = json!({"completed": 4, "total": 6, "current_step": "原型交互验收"});
    }
    json!({"definition_id": id, "name": name, "profile": "Codex", "card": card})
}

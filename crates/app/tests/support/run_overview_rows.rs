//! Stable overview fixtures shared by the presentation regression tests.
use norma::mobile::{
    RunOverview, RunOverviewAgent, RunOverviewAttention, RunOverviewAttentionKind,
    RunOverviewPresence, RunOverviewProject, TaskProgress, WorkspaceActivityStatus,
};

pub(super) fn sample(now: u64) -> RunOverview {
    RunOverview {
        attention: vec![
            attention(
                RunOverviewAttentionKind::Approval,
                "抽屉视觉方案",
                1,
                true,
                "run-1",
                "turn-1",
                vec![],
            ),
            attention(
                RunOverviewAttentionKind::Review,
                "布局验收结果",
                1,
                true,
                "run-r",
                "turn-r",
                vec!["docs/prototypes/norma-assistant/index.html".into()],
            ),
            attention(
                RunOverviewAttentionKind::Review,
                "多文件成果",
                1,
                true,
                "run-m",
                "turn-m",
                vec!["a.rs".into(), "b.rs".into()],
            ),
            attention(
                RunOverviewAttentionKind::File,
                "index.html",
                1,
                true,
                "",
                "turn-f",
                vec!["docs/prototypes/norma-assistant/index.html".into()],
            ),
        ],
        running: vec![
            agent(
                "Bella",
                1,
                RunOverviewPresence::Running,
                Some(progress()),
                Some(now - 1_080_000),
                1,
                Some("调整原型布局与透明材质"),
            ),
            agent(
                "Rio",
                2,
                RunOverviewPresence::Running,
                None,
                Some(now - 30_000),
                0,
                Some("读取画布"),
            ),
        ],
        idle: vec![agent(
            "Bruno",
            8,
            RunOverviewPresence::Idle,
            None,
            None,
            0,
            Some("检查抽屉响应式布局"),
        )],
        unloaded_projects: vec![RunOverviewProject {
            area_id: "area-other".into(),
            order: 2,
            name: "设计草稿".into(),
            agent_count: 1,
            status: WorkspaceActivityStatus::Idle,
            loaded: false,
            updated_at_unix_ms: Some((now - 720_000) as i64),
        }],
    }
}

fn attention(
    kind: RunOverviewAttentionKind,
    title: &str,
    node_id: u64,
    loaded: bool,
    run_id: &str,
    turn_id: &str,
    paths: Vec<String>,
) -> RunOverviewAttention {
    RunOverviewAttention {
        kind,
        area_id: "area-current".into(),
        node_id,
        project: "Norma-AI-OS".into(),
        agent: "Bella".into(),
        title: title.into(),
        run_id: run_id.into(),
        turn_id: turn_id.into(),
        request_id: None,
        fingerprint: Some("fp".into()),
        paths,
        loaded,
    }
}

fn agent(
    name: &str,
    node_id: u64,
    presence: RunOverviewPresence,
    progress: Option<TaskProgress>,
    started: Option<u64>,
    approvals: u32,
    task: Option<&str>,
) -> RunOverviewAgent {
    RunOverviewAgent {
        area_id: "area-current".into(),
        project: "Norma-AI-OS".into(),
        current: true,
        node_id,
        name: name.into(),
        raw_state: "thinking".into(),
        presence,
        approval_count: approvals,
        task: task.map(str::to_string),
        latest_prompt: None,
        action: task.map(str::to_string),
        progress,
        started_at_unix_ms: started,
    }
}

fn progress() -> TaskProgress {
    TaskProgress {
        completed: 4,
        total: 6,
        current_step: Some("原型交互验收".into()),
    }
}

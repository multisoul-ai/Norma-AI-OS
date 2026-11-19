use super::*;
use crate::island::IslandProject;
use norma::mobile::{
    RunOverview, RunOverviewAgent, RunOverviewAttention, RunOverviewAttentionKind,
    RunOverviewPresence,
};

fn agent(name: &str, action: &str) -> RunOverviewAgent {
    RunOverviewAgent {
        area_id: "area".into(),
        project: "Web".into(),
        current: false,
        node_id: 1,
        name: name.into(),
        profile: "codex".into(),
        raw_state: "running".into(),
        presence: RunOverviewPresence::Running,
        approval_count: 0,
        task: Some("重构登录".into()),
        latest_prompt: None,
        action: Some(action.into()),
        last_tool: None,
        progress: None,
        started_at_unix_ms: None,
    }
}

fn workspace(overview: RunOverview) -> IslandWorkspace {
    IslandWorkspace {
        database: "/test/app.db".into(),
        id: "ws".into(),
        name: "Alpha".into(),
        projects: vec![IslandProject {
            source: 1,
            area_id: "area".into(),
            name: "Web".into(),
            root: "/test/web".into(),
            loaded: true,
            cached_overview: overview.clone(),
            overview,
            approvals: Vec::new(),
            questions: Vec::new(),
            proposals: Vec::new(),
        }],
    }
}

/// 场景：汇报概览跨工作空间汇总失败、运行中的事项，并按项目标注来源。
/// 数据：Alpha/Web 有一个失败的 Alan、一个运行中的 Bella（正在 cargo test）。
/// 预期：每条带“Alpha/Web”；失败行含 Alan 与标题；运行行含 Bella 与动作；无空条目。
#[test]
fn overview_entries_cover_failures_and_running_agents_with_their_workspace() {
    let overview = RunOverview {
        attention: vec![RunOverviewAttention {
            kind: RunOverviewAttentionKind::Failed,
            area_id: "area".into(),
            node_id: 2,
            project: "Web".into(),
            agent: "Alan".into(),
            title: "登录接口失败".into(),
            run_id: String::new(),
            turn_id: String::new(),
            request_id: None,
            fingerprint: None,
            paths: Vec::new(),
            loaded: true,
        }],
        running: vec![agent("Bella", "cargo test")],
        idle: Vec::new(),
        unloaded_projects: Vec::new(),
    };
    let entries = overview_entries(&[workspace(overview)]);
    assert_eq!(entries.len(), 2, "失败与运行各一条：{entries:?}");
    assert!(entries
        .iter()
        .all(|e| e.workspace == "Alpha" && e.project == "Web"));
    assert!(
        entries[0].line.contains("Alan") && entries[0].line.contains("登录接口失败"),
        "{:?}",
        entries[0]
    );
    assert!(
        entries[1].line.contains("Bella") && entries[1].line.contains("cargo test"),
        "{:?}",
        entries[1]
    );
}

/// 场景：没有任何事项时不能把空概览交给大脑，汇报按钮应回到“没有可汇报内容”。
/// 预期：空工作空间得到空列表。
#[test]
fn empty_workspaces_produce_no_overview_entries() {
    let empty = RunOverview {
        attention: Vec::new(),
        running: Vec::new(),
        idle: Vec::new(),
        unloaded_projects: Vec::new(),
    };
    assert!(overview_entries(&[workspace(empty)]).is_empty());
}

/// 场景：回复必须到达状态，汇报请求后的回复触发朗读并生成字幕，普通回复只有文字。
/// 预期：汇报后第一条回复 speak=true 且字幕按句；第二条回复 speak=false 且无字幕。
#[test]
fn replies_land_in_the_chat_and_only_requested_reports_get_captions() {
    let mut state = StaffState::default();
    let request = state.begin_report().expect("首次汇报请求应被接受");
    let first = state
        .deliver_reply("Alan 完成了登录。Bella 在测试。", Some(request))
        .expect("回复入对话");
    assert!(first.speak, "汇报请求后的回复必须朗读");
    assert_eq!(state.caption_sentences().len(), 2, "应按句生成字幕");
    let second = state.deliver_reply("另一件事", None).expect("回复入对话");
    assert!(!second.speak);
    assert!(
        state.caption_sentences().is_empty(),
        "未朗读的回复不显示字幕"
    );
    assert_eq!(state.chat.lines().count(), 2);
}

/// 场景：汇报回复没有真正进入朗读队列（语音不可用/失效）时，不能留下字幕，并提示仅显示文字。
/// 预期：先有字幕；end_speech(false) 后字幕清空、对话保留、notice 为 island.staff_speech_unavailable；
/// end_speech(true) 保留字幕且不出现提示。
#[test]
fn captions_only_stay_when_speech_was_actually_queued() {
    let mut state = StaffState::default();
    let request = state.begin_report().expect("请求");
    state
        .deliver_reply("Alan 完成了登录。", Some(request))
        .expect("回复入对话");
    state.end_speech(true);
    assert!(
        !state.caption_sentences().is_empty(),
        "已排队朗读应保留字幕"
    );
    assert!(state.notice.is_none());
    let request = state.begin_report().expect("请求");
    state
        .deliver_reply("Bella 在测试。", Some(request))
        .expect("回复入对话");
    state.end_speech(false);
    assert!(state.caption_sentences().is_empty(), "未能朗读不得显示字幕");
    assert_eq!(state.chat.lines().count(), 2, "文字回复仍保留");
    assert_eq!(
        state.notice,
        Some(ui::i18n::LocalizedText::new(
            "island.staff_speech_unavailable"
        ))
    );
}

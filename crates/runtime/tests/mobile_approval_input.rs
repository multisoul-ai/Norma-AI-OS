use canvas::{AgentProfile, CanvasRect};
use norma::{
    agent_hook::{parse_hook_event, HookSource},
    mobile::{TerminalInput, TerminalRequest},
};
use runtime::{AgentTaskState, AgentTaskUpdate, NodeInput, Runtime};
use serde_json::json;
use std::sync::{Arc, Mutex};

const CLAUDE_MENU: &str = "Bash command\r\n true\r\n Check\r\n Do you want to proceed?\r\n ❯ 1. Yes\r\n 2. Yes, always allow\r\n 3. No\r\n Esc to cancel · Tab to amend · ctrl+e to explain";
const CODEX_MENU: &str = "Would you like to run the following command?\r\n Environment: local\r\n Reason: Check\r\n $ true\r\n › 1. Yes, proceed (y)\r\n 2. No, and tell Codex what to do differently (esc)\r\n Press enter to confirm or esc to cancel";

struct Fixture {
    runtime: Runtime,
    node: u64,
    profile: AgentProfile,
    output: crossbeam_channel::Sender<terminal::PtyEvent>,
}

impl Fixture {
    fn new(profile: AgentProfile) -> Self {
        let output = Arc::new(Mutex::new(None));
        let sender = output.clone();
        let mut runtime = Runtime::new_for_workspace("/tmp");
        runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
            *sender.lock().expect("输出") = Some(tx.clone());
            Ok(Box::new(terminal::TestPtyBackend::recording(
                tx,
                Arc::new(Mutex::new(Vec::new())),
            )))
        }));
        // 100×40 沿用已有真实授权菜单测试尺寸，容纳完整菜单和页脚，不产生截断。
        let node = runtime
            .spawn_agent(profile, CanvasRect::new(0., 0., 640., 400.), 100, 40)
            .expect("原生卡");
        let mut fixture = Self {
            runtime,
            node,
            profile,
            output: output.lock().expect("输出").take().expect("原PTY"),
        };
        fixture.hook("SessionStart", AgentTaskState::Ready);
        fixture.hook("UserPromptSubmit", AgentTaskState::Thinking);
        fixture.hook("PreToolUse", AgentTaskState::Thinking);
        fixture.hook(
            "PermissionRequest",
            AgentTaskState::WaitingApproval {
                tool_name: Some("Bash".into()),
            },
        );
        fixture.screen(match profile {
            AgentProfile::Claude => CLAUDE_MENU,
            _ => CODEX_MENU,
        });
        fixture
    }

    fn hook(&mut self, name: &str, state: AgentTaskState) {
        let source = match self.profile {
            AgentProfile::Claude => HookSource::Claude,
            _ => HookSource::Codex,
        };
        let event = parse_hook_event(
            source,
            self.node,
            json!({
                "hook_event_name":name,"session_id":"menu-session","prompt_id":"turn-a",
                "turn_id":"turn-a","tool_use_id":"tool-a","tool_name":"Bash",
                "tool_input":{"command":"true","description":"Check"}
            }),
        )
        .expect("准确Hook");
        assert!(
            self.runtime.observe_mobile_hook(&event),
            "同会话事件必须接受: {name}"
        );
        self.runtime
            .apply_agent_task_update_with_outcome(AgentTaskUpdate {
                node_id: self.node,
                profile: self.profile,
                provider_event: name.into(),
                to: state,
                task_title: None,
                session_id: Some("menu-session".into()),
            });
    }

    fn screen(&mut self, text: &str) {
        self.output
            .send(terminal::PtyEvent::Output(
                format!("\x1b[2J\x1b[H{text}").into_bytes(),
            ))
            .expect("菜单输出");
        self.runtime.drain_all();
    }

    fn phone(&mut self, input: TerminalInput) {
        let card = self.runtime.mobile_card_snapshots().remove(0);
        let page = self
            .runtime
            .mobile_terminal_page(&TerminalRequest {
                workspace_id: "workspace".into(),
                route_id: "route".into(),
                node_id: self.node,
                run_id: card.run_id.expect("运行"),
                session_id: card.session_id,
                before: None,
                revision: None,
            })
            .expect("当前手机终端");
        self.runtime
            .send_mobile_terminal_input(
                &page.card,
                page.screen_signature,
                page.input_sequence,
                &input,
            )
            .expect("手机按键");
    }

    fn finish(&mut self) -> Result<(), &'static str> {
        self.hook("PostToolUse", AgentTaskState::Thinking);
        self.hook("Stop", AgentTaskState::Done);
        let card = self.runtime.mobile_card_snapshots().remove(0);
        assert_eq!(card.state, "done", "任务结束状态必须已确认");
        self.runtime.validate_mobile_message(&card, "继续对话")
    }
}

/// Claude 菜单方向键只改变选项，确认后不应留下不存在的聊天草稿。
///
/// 数据构造：1 张原生 Claude 卡、完整授权菜单；Down/Up/Enter 共 3 个按键，无正文。
/// 执行过程：准确工具确认 → 手机 Down → 电脑 Up → 手机 Enter → PostToolUse/Stop → 校验。
/// 预期结果：完成后可发消息；没有第二个 PTY；随后真实桌面正文仍被保护。
#[test]
fn claude_menu_navigation_does_not_block_chat() {
    let mut f = Fixture::new(AgentProfile::Claude);
    f.phone(TerminalInput::Down);
    f.runtime
        .send_node_input(
            f.node,
            NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Up,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        )
        .expect("电脑菜单导航");
    f.phone(TerminalInput::Enter);
    assert_eq!(f.finish(), Ok(()), "手机和电脑的菜单导航不能留下草稿误报");
    assert_eq!(f.runtime.live_len(), 1, "菜单处理不能另开执行器");
    f.runtime
        .send_node_input(f.node, NodeInput::Text("真实草稿".into()))
        .expect("电脑正文");
    let card = f.runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        f.runtime.validate_mobile_message(&card, "手机消息"),
        Err("desktop_input_pending"),
        "修复不得覆盖真实草稿"
    );
}

/// Codex 的同类授权导航沿用相同规则，不会因为多次方向键重新被标记为草稿。
///
/// 数据构造：1 张原生 Codex 卡、同一回合内两次完整菜单；每次 Down/Up/Enter，共 6 键，无正文。
/// 执行过程：手机确认首个菜单 → 工具完成 → 新菜单 Hook → 再次确认 → 校验 → 空闲时 Up。
/// 预期结果：两次确认后可继续聊天；空闲时的历史召回仍须阻止手机覆盖。
#[test]
fn codex_menu_navigation_does_not_block_chat() {
    let mut f = Fixture::new(AgentProfile::Codex);
    f.phone(TerminalInput::Down);
    f.phone(TerminalInput::Up);
    f.phone(TerminalInput::Enter);
    f.hook("PostToolUse", AgentTaskState::Thinking);
    f.hook("PreToolUse", AgentTaskState::Thinking);
    f.hook(
        "PermissionRequest",
        AgentTaskState::WaitingApproval {
            tool_name: Some("Bash".into()),
        },
    );
    f.phone(TerminalInput::Down);
    f.phone(TerminalInput::Up);
    f.phone(TerminalInput::Enter);
    assert_eq!(f.finish(), Ok(()), "Codex 菜单也不能残留草稿误报");
    f.phone(TerminalInput::Up);
    let card = f.runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        f.runtime.validate_mobile_message(&card, "手机消息"),
        Err("desktop_input_pending"),
        "任务结束后不能继续把方向键视为菜单操作"
    );
}

/// 迟到的授权状态不代表当前画面仍是菜单，普通聊天框的历史召回必须保守处理。
///
/// 数据构造：Claude 仍为 WaitingApproval，但终端已重绘为空聊天框；1 次 Up 可能召回正文。
/// 执行过程：替换菜单画面 → 手机 Up/Enter → 迟到 PostToolUse/Stop → 校验。
/// 预期结果：历史召回不能冒充菜单导航；工具完成不能清除这份未知草稿。
#[test]
fn stale_approval_state_does_not_clear_recalled_history() {
    let mut f = Fixture::new(AgentProfile::Claude);
    f.screen("Claude\r\n❯ ");
    f.phone(TerminalInput::Up);
    f.phone(TerminalInput::Enter);
    assert_eq!(
        f.finish(),
        Err("desktop_input_pending"),
        "只凭过期审批状态不能放行未知正文"
    );
}

/// Enter 已关闭菜单而 PTY 尚未重绘时，后续方向键不能沿用旧画面的导航判断。
///
/// 数据构造：Claude 完整菜单；Down/Enter 后故意不发送新画面，随后 Up 可能召回历史。
/// 执行过程：手机确认菜单 → 旧画面仍在 → 手机 Up → 迟到工具完成 → 校验。
/// 预期结果：确认之前的 Down 不算草稿，确认之后的 Up 保留未知输入保护。
#[test]
fn menu_confirmation_invalidates_navigation_before_redraw() {
    let mut f = Fixture::new(AgentProfile::Claude);
    f.phone(TerminalInput::Down);
    f.phone(TerminalInput::Enter);
    f.phone(TerminalInput::Up);
    assert_eq!(
        f.finish(),
        Err("desktop_input_pending"),
        "旧菜单画面不能放行确认后的历史召回"
    );
}

/// 菜单处理前已有的草稿与确认后新输入的草稿都不能被授权完成事件清除。
///
/// 数据构造：两张隔离 Claude 卡；一份草稿在导航前输入，另一份在 Enter 后输入。
/// 执行过程：分别在菜单前后输入正文 → 确认菜单 → 收到工具完成/Stop → 校验。
/// 预期结果：两份真实草稿都拒绝覆盖，不允许用任务结束无条件清零。
#[test]
fn approval_completion_preserves_existing_and_newer_drafts() {
    let mut existing = Fixture::new(AgentProfile::Claude);
    existing
        .runtime
        .send_node_input(existing.node, NodeInput::Text("已有草稿".into()))
        .expect("已有正文");
    existing.phone(TerminalInput::Down);
    existing.phone(TerminalInput::Enter);
    assert_eq!(
        existing.finish(),
        Err("desktop_input_pending"),
        "导航前的草稿必须保留保护"
    );
    let mut newer = Fixture::new(AgentProfile::Claude);
    newer.phone(TerminalInput::Down);
    newer.phone(TerminalInput::Enter);
    newer.phone(TerminalInput::Text {
        text: "下一条草稿".into(),
    });
    assert_eq!(
        newer.finish(),
        Err("desktop_input_pending"),
        "迟到完成事件不能清除新草稿"
    );
}

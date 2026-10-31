use super::*;
use norma::agent_hook::{parse_hook_event, HookSource};
use serde_json::json;

/// Write只在完整原生创建预览和准确目标路径匹配时允许手机单次决策。
///
/// 数据构造：/project下new.txt、两行内容，含4空格缩进；原生1/2/3选择和准确页脚。
/// 执行过程：匹配完整预览→分别替换文件、正文、行号及页脚→检查拒绝未知或截断画面。
/// 预期结果：准确预览返回1/3；不同路径、内容、缺行及未知页脚全部不可操作。
#[test]
fn native_claude_write_dialog_requires_complete_preview() {
    use super::native_approval::write_dialog_keys;
    let lines: Vec<String> = [
        "Create file",
        "new.txt",
        "╌╌╌",
        "1 hello",
        "2     world",
        "╌╌╌",
        "Do you want to create new.txt?",
        "❯ 1. Yes",
        "2. Yes, allow all edits in project/ during this session (shift+tab)",
        "3. No",
        "Esc to cancel · Tab to amend",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let input = json!({"file_path":"/project/new.txt", "content":"hello\n    world\n"});
    let check = |screen: &[String], input: &serde_json::Value| {
        write_dialog_keys(screen, input, Path::new("/project"))
    };
    assert_eq!(
        check(&lines, &input),
        Some(('1', '3')),
        "完整预览才能得到单次批准和拒绝快捷键"
    );
    assert_eq!(
        check(
            &lines,
            &json!({"file_path":"/other/new.txt", "content":"hello\n    world\n"})
        ),
        None,
        "同名文件不能匹配别的目录"
    );
    assert_eq!(
        check(
            &lines,
            &json!({"file_path":"/project/new.txt", "content":"hello\nworld\n"})
        ),
        None,
        "代码缩进变化不能被折叠"
    );
    let mut missing = lines.clone();
    missing.remove(4);
    assert_eq!(check(&missing, &input), None, "缺少第二行不能假装完整预览");
    let mut footer = lines.clone();
    *footer.last_mut().expect("页脚") = "Press Enter to trust all".into();
    assert_eq!(check(&footer, &input), None, "未知确认布局不能发送快捷键");
}

fn fixture(path: &Path) -> (Runtime, NodeId) {
    let mut r = Runtime::new_for_workspace(path);
    r.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    // Existing card geometry and 100×40 terminal match the real Claude approval probe.
    let node = r
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0., 0., 640., 400.),
            100,
            40,
        )
        .expect("原卡");
    std::fs::write(path.join("session-a.jsonl"), "").expect("隔离会话");
    hook(&mut r, node, path, "UserPromptSubmit", "tool-a");
    hook(&mut r, node, path, "PreToolUse", "tool-a");
    hook(&mut r, node, path, "PermissionRequest", "tool-a");
    r.apply_agent_task_update_with_outcome(crate::AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: "PermissionRequest".into(),
        to: crate::AgentTaskState::WaitingApproval {
            tool_name: Some("Bash".into()),
        },
        task_title: None,
        session_id: Some("session-a".into()),
    });
    screen(&mut r, node, "Bash command\r\n\r\n  printf approval\r\n  Write marker\r\n\r\n Do you want to proceed?\r\n ❯ 1. Yes\r\n   2. Yes, and always allow access\r\n   3. No\r\n\r\n Esc to cancel · Tab to amend · ctrl+e to explain");
    (r, node)
}

fn hook(r: &mut Runtime, node: NodeId, path: &Path, name: &str, tool: &str) {
    let mut data = json!({"hook_event_name":name,"session_id":"session-a","prompt_id":"turn-a",
        "transcript_path":path.join("session-a.jsonl"),"tool_name":"Bash",
        "tool_input":{"command":"printf approval","description":"Write marker"}});
    if name != "PermissionRequest" {
        data["tool_use_id"] = json!(tool);
    }
    let event = parse_hook_event(HookSource::Claude, node, data).expect("原生事件");
    assert!(r.observe_mobile_hook(&event), "准确会话事件应接收");
}

fn screen(r: &mut Runtime, node: NodeId, text: &str) {
    let live = r.live.get_mut(&node).expect("原终端");
    live.input(crate::NodeInput::Text(format!("\x1b[2J\x1b[H{text}")))
        .expect("输出屏幕");
    live.drain();
}

/// 原卡单个准确请求在原生确认屏幕出现后可决策；电脑先输入会立即废弃手机目标。
///
/// 数据构造：session-a/turn-a、唯一tool-a；PermissionRequest缺少tool ID，精确输入与PreToolUse配对。
/// 执行过程：生成原生快照 → 核对工具和决定 → 电脑输入Down → 再读取及尝试旧决定。
/// 预期结果：初始显示Bash批准/拒绝；电脑操作后不可复用旧批准；保留原卡和唯一PTY。
#[test]
fn native_claude_approval_is_bound_and_desktop_input_invalidates_it() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (mut r, node) = fixture(dir.path());
    let snapshot = r.mobile_approval_snapshots(node).expect("审批").remove(0);
    assert_eq!(
        r.cached_mobile_approval_snapshots(node).expect("缓存审批"),
        [snapshot.clone()],
        "概览读取不得丢失原生请求身份，也不得推进 Provider 或审计 IO"
    );
    assert_eq!(snapshot.tool, "Bash", "原卡必须显示真实待确认工具");
    assert_eq!(
        snapshot.details["command"], "printf approval",
        "只允许准确工具输入"
    );
    assert_eq!(
        snapshot.decisions,
        vec![ApprovalDecision::Allow, ApprovalDecision::Deny],
        "准确原生请求应可操作"
    );
    r.send_node_input(
        node,
        crate::NodeInput::Key(terminal::KeyInput {
            key: terminal::Key::Down,
            ctrl: false,
            alt: false,
            shift: false,
        }),
    )
    .expect("电脑保持原生操作");
    assert!(
        r.mobile_approval_snapshots(node)
            .expect("新快照")
            .iter()
            .all(|s| s.decisions.is_empty()),
        "电脑先操作后不得残留手机按钮"
    );
    assert_eq!(
        r.choose_mobile_approval(&snapshot.target, ApprovalDecision::Allow),
        Err("approval_changed"),
        "旧手机请求不能操作电脑当前选择"
    );
    assert_eq!(r.live_len(), 1, "不能建立第二执行器");
    assert!(
        matches!(&r.document.node(node).expect("原卡").kind, CanvasNodeKind::Agent(s) if !s.mobile_control),
        "不得修改原卡模式"
    );
}

/// 并发同输入和未显示原生确认界面时，不能凭Hook或文本猜测审批目标。
///
/// 数据构造：单请求fixture；先把屏幕替换为普通正文，再添加相同输入的tool-b和第二PermissionRequest。
/// 执行过程：正文阶段读取 → 并发同输入 → 恢复看似正确的确认屏幕。
/// 预期结果：两个阶段都没有可执行决定，不能把tool-b当作tool-a。
#[test]
fn native_claude_approval_rejects_missing_dialog_and_ambiguous_tool() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (mut r, node) = fixture(dir.path());
    screen(&mut r, node, "Claude is thinking: Yes / No");
    let preparing = r.mobile_approval_snapshots(node).expect("菜单尚未就绪");
    assert_eq!(
        preparing[0].tool, "native_terminal",
        "无动作的过渡状态必须符合Swift的只读提示协议"
    );
    assert!(
        preparing[0].details.get("command").is_none(),
        "尚未核验菜单不能发布可决策Bash正文"
    );
    assert!(
        r.mobile_approval_snapshots(node)
            .expect("正文")
            .iter()
            .all(|s| s.decisions.is_empty()),
        "普通正文不是原生确认"
    );
    screen(&mut r, node, "Bash command\r\n printf approval-unexpected\r\n Write marker\r\n Do you want to proceed?\r\n ❯ 1. Yes\r\n 2. Yes, and always allow access\r\n 3. No\r\n Esc to cancel · Tab to amend · ctrl+e to explain");
    assert!(
        r.mobile_approval_snapshots(node)
            .expect("其他命令")
            .iter()
            .all(|s| s.decisions.is_empty()),
        "命令前缀相同不能操作另一个请求"
    );
    screen(&mut r, node, "Bash command\r\n printfapproval\r\n Write marker\r\n Do you want to proceed?\r\n ❯ 1. Yes\r\n 2. Yes, and always allow access\r\n 3. No\r\n Esc to cancel · Tab to amend · ctrl+e to explain");
    assert!(
        r.mobile_approval_snapshots(node)
            .expect("不同分词")
            .iter()
            .all(|s| s.decisions.is_empty()),
        "不能删除命令内部空格后判断相等"
    );
    hook(&mut r, node, dir.path(), "PreToolUse", "tool-b");
    hook(&mut r, node, dir.path(), "PermissionRequest", "tool-b");
    screen(&mut r, node, "Bash command\r\n printf approval\r\n Write marker\r\n Do you want to proceed?\r\n ❯ 1. Yes\r\n 2. Yes, and always allow access\r\n 3. No\r\n Esc to cancel · Tab to amend · ctrl+e to explain");
    assert!(
        r.mobile_approval_snapshots(node)
            .expect("多请求")
            .iter()
            .all(|s| s.decisions.is_empty()),
        "无唯一tool ID不能批准"
    );
}

/// 原生手机拒绝必须等到准确新tool_result，旧记录和其他工具不算回执。
///
/// 数据构造：唯一tool-a、session-a/turn-a；决定前旧tool-a记录，决定后tool-b及准确tool-a各一条。
/// 执行过程：预约 → 捕获EOF → 原PTY快捷键 → 读取无结果 → 追加其他工具 → 追加准确拒绝 → 完成。
/// 预期结果：按键不能产生完成；只有准确新增拒绝结束本回合，唯一PTY保留且可再次输入。
#[test]
fn native_claude_approval_waits_for_exact_new_result() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (mut r, node) = fixture(dir.path());
    let snapshot = r.mobile_approval_snapshots(node).expect("审批").remove(0);
    append_result(dir.path(), "tool-a");
    let mut request = r
        .reserve_native_approval(&snapshot.target, ApprovalDecision::Deny)
        .expect("预约拒绝");
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut request.history else {
        panic!("Claude reader");
    };
    reader.projects_root = dir.path().into();
    request.prepare().expect("后台捕获末尾");
    assert!(
        r.reserve_native_approval(&snapshot.target, ApprovalDecision::Allow)
            .is_err(),
        "另一台手机不能重复预约"
    );
    r.send_native_approval(&mut request)
        .expect("原PTY精确快捷键");
    assert!(!request.poll().expect("旧结果"), "旧记录不能产生新回执");
    assert!(!r.finish_native_approval(&request), "按键不等于结果");
    append_result(dir.path(), "tool-b");
    assert!(!request.poll().expect("其他工具"), "其他工具不属于本次批准");
    append_result(dir.path(), "tool-a");
    assert!(
        !request.poll().expect("拒绝工具结果"),
        "拒绝后仍需准确中断边界才能结束回合"
    );
    append_interruption(dir.path());
    assert!(request.poll().expect("本工具"), "准确拒绝应产生回执");
    assert!(r.finish_native_approval(&request), "可以结束准确拒绝回合");
    assert_eq!(
        r.agent_task_state(node),
        Some(crate::AgentTaskState::Ended),
        "原生No结束本回合"
    );
    assert_eq!(r.live_len(), 1, "原生CLI不能被替换或退出");
    assert!(
        r.mobile_approval_snapshots(node)
            .expect("完成后")
            .is_empty(),
        "完成请求不能残留"
    );
    assert_eq!(
        r.validate_mobile_message(&r.mobile_card_snapshots().remove(0), "继续"),
        Ok(()),
        "手机快捷键不能污染电脑草稿"
    );
}

fn append_interruption(path: &Path) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path.join("session-a.jsonl"))
        .expect("原历史");
    writeln!(file, "{}", json!({"type":"user","uuid":"interrupted","sessionId":"session-a","cwd":path,"promptId":"turn-a",
        "message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}})).expect("准确中断边界");
}

fn append_result(path: &Path, tool: &str) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path.join("session-a.jsonl"))
        .expect("原历史");
    writeln!(file, "{}", json!({"type":"user","uuid":format!("result-{tool}"),"sessionId":"session-a","cwd":path,"promptId":"turn-a",
        "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":tool,"is_error":true,
        "content":"The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). STOP what you are doing and wait for the user to tell you how to proceed."}]}})).expect("Provider结果");
}

/// 准备期间电脑接管时，不得向已变化的原终端发送手机快捷键。
///
/// 数据构造：同一待批tool-a，后台EOF准备完成后电脑输入数字3。
/// 执行过程：预约允许 → 捕获EOF → 电脑拒绝 → 尝试旧手机发送。
/// 预期结果：旧请求明确失效，不得再向原PTY写入1或产生批准成功。
#[test]
fn native_claude_approval_revalidates_after_background_prepare() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (mut r, node) = fixture(dir.path());
    let snapshot = r.mobile_approval_snapshots(node).expect("审批").remove(0);
    let mut request = r
        .reserve_native_approval(&snapshot.target, ApprovalDecision::Allow)
        .expect("预约");
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut request.history else {
        panic!("Claude reader");
    };
    reader.projects_root = dir.path().into();
    request.prepare().expect("准备");
    r.send_node_input(node, crate::NodeInput::Text("3".into()))
        .expect("电脑接管");
    assert!(
        r.send_native_approval(&mut request).is_err(),
        "旧手机请求不能抢占电脑操作"
    );
    assert!(!r.finish_native_approval(&request), "取消不能伪造成功");
}

/// 批准送达与Bash退出状态分开：准确PostToolUseFailure能证明工具已执行。
///
/// 数据构造：原session-a/turn-a/tool-a；原生exit7事件包含完整输入和is_interrupt=false。
/// 执行过程：允许 → 其他工具/中断/不同输入的失败事件 → 精确原生工具失败事件 → 读取回执。
/// 预期结果：前三种事件均不能确认；准确非中断失败应确认submitted，并保持原会话可操作。
#[test]
fn native_claude_failed_bash_still_confirms_delivered_allow() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (mut r, node) = fixture(dir.path());
    let snapshot = r.mobile_approval_snapshots(node).expect("审批").remove(0);
    let mut request = r
        .reserve_native_approval(&snapshot.target, ApprovalDecision::Allow)
        .expect("预约");
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut request.history else {
        panic!("Claude reader");
    };
    reader.projects_root = dir.path().into();
    request.prepare().expect("决定前EOF");
    r.send_native_approval(&mut request).expect("原PTY批准");
    let event = |tool: &str, interrupted: bool, command: &str| {
        parse_hook_event(HookSource::Claude, node, json!({
            "hook_event_name":"PostToolUseFailure", "session_id":"session-a", "prompt_id":"turn-a",
            "transcript_path":dir.path().join("session-a.jsonl"), "tool_use_id":tool, "tool_name":"Bash",
            "tool_input":{"command":command,"description":"Write marker"},
            "error":"Exit code 7", "is_interrupt":interrupted
        })).expect("原生失败事件")
    };
    r.observe_mobile_hook(&event("tool-b", false, "printf approval"));
    assert!(
        !request.poll().expect("其他工具"),
        "其他工具不能确认当前批准"
    );
    r.observe_mobile_hook(&event("tool-a", true, "printf approval"));
    assert!(!request.poll().expect("中断"), "中断不能证明工具开始执行");
    r.observe_mobile_hook(&event("tool-a", false, "printfapproval"));
    assert!(
        !request.poll().expect("不同命令"),
        "不同输入不能证明准确工具执行"
    );
    // A tool_result can arrive before its PostToolUseFailure Hook; keep waiting for execution evidence.
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join("session-a.jsonl"))
        .expect("原历史");
    writeln!(file, "{}", json!({"type":"user","uuid":"failed-tool-a","sessionId":"session-a","cwd":dir.path(),"promptId":"turn-a",
        "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-a","is_error":true,"content":"Exit code 7"}]}})).expect("原生退出码结果");
    assert!(
        !request.poll().expect("失败Hook尚未送达"),
        "工具错误不应提前变成授权结果未知"
    );
    r.observe_mobile_hook(&event("tool-a", false, "printf approval"));
    assert!(
        request.poll().expect("准确失败"),
        "已执行后exit7仍证明批准送达"
    );
    assert_eq!(
        request.result().expect("回执")["status"],
        "submitted",
        "不能将工具失败误报为授权结果未知"
    );
    assert!(r.finish_native_approval(&request), "准确批准应收口");
    assert_eq!(r.live_len(), 1, "失败不替换原CLI");
}

/// 原生Claude shell包装仅在完整argv与审批命令逐字一致时证明工具已启动。
///
/// 数据构造：真实2.1.202 zsh包装，固定snapshot/cwd标识；命令含引号和连续两个空格。
/// 执行过程：匹配准确包装 → 改命令空格/增加尾命令/改snapshot路径/额外argv/换shell。
/// 预期结果：仅原始完整命令匹配，任何相似命令或未验证包装均无效。
#[test]
fn native_bash_process_requires_exact_command_and_wrapper() {
    use native_approval_action::matches_bash_process;
    let root = Path::new("/fixture/.claude/projects");
    let command = "printf 'a  b' > '/tmp/allowed.txt'";
    let script = "source /fixture/.claude/shell-snapshots/snapshot-zsh-1788950745112-l6n7ec.sh 2>/dev/null || true && setopt NO_EXTENDED_GLOB NO_BARE_GLOB_QUAL 2>/dev/null || true && eval 'printf '\"'\"'a  b'\"'\"' > '\"'\"'/tmp/allowed.txt'\"'\"'' < /dev/null && pwd -P >| /tmp/claude-6d4f-cwd";
    let argv = vec!["/bin/zsh".into(), "-c".into(), script.into()];
    assert!(
        matches_bash_process(&argv, command, root),
        "准确原生包装必须匹配"
    );
    assert!(
        !matches_bash_process(&argv, "printf 'a b' > '/tmp/allowed.txt'", root),
        "命令内部空格不能归一化"
    );
    let mut changed = argv.clone();
    changed[2].push_str("; rm -f /tmp/other");
    assert!(
        !matches_bash_process(&changed, command, root),
        "包装尾部额外命令必须拒绝"
    );
    changed[2] = script.replace("1788950745112-l6n7ec", "../../other");
    assert!(
        !matches_bash_process(&changed, command, root),
        "快照不能包含任意路径"
    );
    changed = argv.clone();
    changed.push("extra".into());
    assert!(
        !matches_bash_process(&changed, command, root),
        "必须核对完整argv而非前三项"
    );
    changed = argv.clone();
    changed[0] = "/bin/sh".into();
    assert!(
        !matches_bash_process(&changed, command, root),
        "未验证shell不能猜测"
    );
}

/// 原生多段Bash审批带固定提示和折行的“始终允许”项，仍应可准确选择单次允许/拒绝。
///
/// 数据构造：沿用tool-a；真实屏幕的固定提示一行、第二菜单项两行、数字1/2/3各一个。
/// 执行过程：写入真实长命令布局 → 核对单次决定 → 追加额外数字菜单项再读取。
/// 预期结果：已验证折行布局可操作；未知第四选项不得沿用旧菜单按键。
#[test]
fn native_claude_approval_accepts_wrapped_native_menu() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let (mut r, node) = fixture(dir.path());
    let dialog = "Bash command\r\n printf approval\r\n Write marker\r\n This command requires approval\r\n Do you want to proceed?\r\n ❯ 1. Yes\r\n 2. Yes, and don't ask again for printf and exit commands in\r\n /fixture/project\r\n 3. No\r\n Esc to cancel · Tab to amend · ctrl+e to explain";
    screen(&mut r, node, dialog);
    let snapshot = r
        .mobile_approval_snapshots(node)
        .expect("真实折行菜单")
        .remove(0);
    assert_eq!(snapshot.tool, "Bash", "固定提示和折行不能隐藏准确Bash审批");
    assert_eq!(
        snapshot.decisions,
        [ApprovalDecision::Allow, ApprovalDecision::Deny],
        "只提供单次允许和拒绝"
    );
    screen(
        &mut r,
        node,
        &dialog.replace("/fixture/project", "4. Different action"),
    );
    let unknown = r
        .mobile_approval_snapshots(node)
        .expect("未知菜单")
        .remove(0);
    assert!(unknown.decisions.is_empty(), "未知菜单不能沿用旧按键");
    assert_ne!(unknown.tool, "Bash", "未知菜单必须是有效只读提示");
}

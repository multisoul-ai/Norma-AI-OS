use norma::cli::{parse_invocation_from, OutputFormat};
use norma::protocol::{CardCommand, EgoCommand, PresentationTarget};

/// Ego CLI：展示命令与生命周期命令应映射为稳定的公开协议。
///
/// 数据构造（含关键数值的推导过程）：
///   present argv = 4 个参数：norma + present + ego + --new
///   status argv  = 4 个参数：norma + ego + status + node_id(42)
///   handoff argv = 6 个参数：norma + ego + handoff + node_id(42) + --message + 文本
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 `present ego --new` → 生成新的 Ego 展示请求
///   2. 解析 `ego status 42` → 查询同一节点的状态
///   3. 解析 `ego handoff 42 --message ...` → 把控制权显式交给用户
///   4. 检查三条自动化命令均默认使用 JSON 输出
///
/// 预期结果：
///   - 正断言：present 目标为 Ego 且 create_new=true
///   - 正断言：status 与 handoff 均保留精确 node_id=42
///   - 正断言：handoff 保留完整消息
///   - 负断言：present 不应退化为普通 Browser
#[test]
fn test_parse_ego_presentation_and_lifecycle_commands() {
    let present = parse_invocation_from(["norma", "present", "ego", "--new"])
        .expect("present ego --new 应解析成功");
    let status =
        parse_invocation_from(["norma", "ego", "status", "42"]).expect("ego status 应解析成功");
    let handoff = parse_invocation_from([
        "norma",
        "ego",
        "handoff",
        "42",
        "--message",
        "请检查登录结果",
    ])
    .expect("ego handoff 应解析成功");

    assert_eq!(
        present.command,
        CardCommand::Present {
            target: PresentationTarget::Ego,
            create_new: true,
        },
        "present ego --new 必须生成独立 Ego 展示请求"
    );
    assert_ne!(
        present.command,
        CardCommand::Present {
            target: PresentationTarget::Browser { url: String::new() },
            create_new: true,
        },
        "Ego 展示不应退化成 Norma 原生 Browser"
    );
    assert_eq!(
        status.command,
        CardCommand::Ego {
            node_id: 42,
            command: EgoCommand::Status,
        },
        "ego status 必须查询精确的节点 42"
    );
    assert_eq!(
        handoff.command,
        CardCommand::Ego {
            node_id: 42,
            command: EgoCommand::Handoff {
                message: "请检查登录结果".to_string(),
            },
        },
        "ego handoff 必须保留节点和完整交接消息"
    );
    assert_eq!(
        present.format,
        OutputFormat::Json,
        "present ego 自动化命令应默认输出 JSON"
    );
    assert_eq!(
        status.format,
        OutputFormat::Json,
        "ego status 自动化命令应默认输出 JSON"
    );
    assert_eq!(
        handoff.format,
        OutputFormat::Json,
        "ego handoff 自动化命令应默认输出 JSON"
    );
}

/// Ego CLI：Agent 工作态、完成态与失败态必须提供显式协议命令。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 7（同一张 Ego 卡片）
///   running argv  = 4 个参数：norma + ego + running + 7
///   complete argv = 4 个参数：norma + ego + complete + 7
///   fail argv     = 6 个参数：norma + ego + fail + 7 + --message + 原因
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别解析 running、complete、fail
///   2. 检查三条命令都绑定到 node_id=7
///   3. 检查 fail 原因未被丢弃
///
/// 预期结果：
///   - 正断言：生成 Running、Complete、Fail 三类不同命令
///   - 正断言：Fail 保留精确错误消息
///   - 负断言：Complete 不应误解析成 Running
#[test]
fn test_parse_ego_agent_progress_commands() {
    let running =
        parse_invocation_from(["norma", "ego", "running", "7"]).expect("ego running 应解析成功");
    let complete =
        parse_invocation_from(["norma", "ego", "complete", "7"]).expect("ego complete 应解析成功");
    let failed = parse_invocation_from(["norma", "ego", "fail", "7", "--message", "页面验证失败"])
        .expect("ego fail 应解析成功");

    assert_eq!(
        running.command,
        CardCommand::Ego {
            node_id: 7,
            command: EgoCommand::Running,
        },
        "ego running 必须标记节点 7 正由 Agent 工作"
    );
    assert_eq!(
        complete.command,
        CardCommand::Ego {
            node_id: 7,
            command: EgoCommand::Complete,
        },
        "ego complete 必须完成节点 7 的任务"
    );
    assert_ne!(
        complete.command, running.command,
        "完成命令不应误解析成 Agent 工作命令"
    );
    assert_eq!(
        failed.command,
        CardCommand::Ego {
            node_id: 7,
            command: EgoCommand::Fail {
                message: "页面验证失败".to_string(),
            },
        },
        "ego fail 必须保留节点 7 和精确失败原因"
    );
}

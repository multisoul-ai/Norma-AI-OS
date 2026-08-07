use super::{terminal_wheel_action, TerminalGrid, TerminalWheelAction};

/// OpenCode 这类 TUI 进入 alternate screen 并启用 SGR mouse 后，snapshot 应暴露这些模式。
///
/// 数据构造（含关键数值推导）：
///   grid = 80 cols × 24 lines
///   ESC[?1049h = 进入 alternate screen
///   ESC[?1000h = 启用 mouse click report，因此 TermMode::MOUSE_MODE 应为 true
///   ESC[?1006h = 启用 SGR mouse 坐标编码
///
/// 执行过程：
///   1. 初始 snapshot，确认未进入 TUI mouse 模式
///   2. advance 三个 DEC private mode set 序列
///   3. 再次 snapshot，读取 alt_screen / mouse_mode / sgr_mouse
///
/// 预期结果：
///   - 正断言：进入 TUI 后 alt_screen、mouse_mode、sgr_mouse 均为 true
///   - 负断言：初始 snapshot 不应已经处于 alt_screen 或 mouse mode
#[test]
fn test_snapshot_exposes_alt_screen_and_mouse_modes() {
    let mut grid = TerminalGrid::new(80, 24);
    let initial = grid.snapshot();

    grid.advance(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h");
    let tui = grid.snapshot();

    assert!(
        !initial.alt_screen,
        "初始 terminal 不应处于 alt screen，否则普通 shell scrollback 会被误判为 TUI"
    );
    assert!(
        !initial.mouse_mode,
        "初始 terminal 不应启用 mouse mode，否则普通 shell 滚轮会被误转发"
    );
    assert!(
        !initial.sgr_mouse,
        "初始 terminal 不应启用 SGR mouse，否则普通 shell 会生成 mouse escape"
    );
    assert!(tui.alt_screen, "ESC[?1049h 后 snapshot 应标记 alt_screen");
    assert!(tui.mouse_mode, "ESC[?1000h 后 snapshot 应标记 mouse_mode");
    assert!(tui.sgr_mouse, "ESC[?1006h 后 snapshot 应标记 sgr_mouse");
}

/// 普通 terminal 屏幕下 wheel action 必须保持本地 scrollback，不向 PTY 写 TUI mouse bytes。
///
/// 数据构造（含关键数值推导）：
///   grid = 4 cols × 3 lines
///   输入 5 行 → history_size = 5 - 3 = 2
///   wheel lines = +2，普通屏幕没有 alt_screen，因此应本地回看 2 行
///
/// 执行过程：
///   1. 写入 5 行形成 scrollback
///   2. snapshot 读取 terminal mode
///   3. terminal_wheel_action(snapshot, +2, row=1, col=2)
///
/// 预期结果：
///   - 正断言：返回 Scrollback(2)，保留现有 Terminal/Claude/Codex scrollback
///   - 负断言：不应返回 WriteBytes，否则普通 terminal 滚轮会被错误转发给 shell
#[test]
fn test_wheel_action_preserves_scrollback_outside_alt_screen() {
    let mut grid = TerminalGrid::new(4, 3);
    grid.advance(b"A0\r\nB1\r\nC2\r\nD3\r\nE4");
    let snapshot = grid.snapshot();

    let action = terminal_wheel_action(&snapshot, 2, 1, 2);

    assert_eq!(
        action,
        TerminalWheelAction::Scrollback(2),
        "普通屏幕 wheel 应保持本地 scrollback 行为"
    );
    assert_ne!(
        action,
        TerminalWheelAction::WriteBytes(b"\x1b[<64;3;2M".to_vec()),
        "普通屏幕不应生成 SGR mouse bytes，否则 shell scrollback 会失效"
    );
}

/// alternate screen + SGR mouse 模式下 wheel action 应生成 xterm SGR mouse wheel 序列。
///
/// 数据构造（含关键数值推导）：
///   row=2, col=4 是 0-based grid 坐标
///   SGR mouse 使用 1-based 坐标，因此 x=col+1=5, y=row+1=3
///   wheel up button code = 64；wheel down button code = 65
///
/// 执行过程：
///   1. 启用 ?1049h、?1000h、?1006h
///   2. 分别生成 +1 与 -1 行 wheel action
///   3. 检查写入 PTY 的 bytes
///
/// 预期结果：
///   - 正断言：向上为 ESC[<64;5;3M，向下为 ESC[<65;5;3M
///   - 负断言：序列不应使用 0-based 坐标 4;2
#[test]
fn test_wheel_action_encodes_sgr_mouse_in_alt_screen() {
    let mut grid = TerminalGrid::new(80, 24);
    grid.advance(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h");
    let snapshot = grid.snapshot();

    let up = terminal_wheel_action(&snapshot, 1, 2, 4);
    let down = terminal_wheel_action(&snapshot, -1, 2, 4);

    assert_eq!(
        up,
        TerminalWheelAction::WriteBytes(b"\x1b[<64;5;3M".to_vec()),
        "TUI wheel up 应生成 SGR mouse button 64 且使用 1-based 坐标"
    );
    assert_eq!(
        down,
        TerminalWheelAction::WriteBytes(b"\x1b[<65;5;3M".to_vec()),
        "TUI wheel down 应生成 SGR mouse button 65 且使用 1-based 坐标"
    );
    assert_ne!(
        up,
        TerminalWheelAction::WriteBytes(b"\x1b[<64;4;2M".to_vec()),
        "SGR mouse 不应使用 0-based 坐标，否则 TUI 命中位置会偏移"
    );
}

/// alternate screen 未启用 mouse mode 时，alternate scroll 应退化为方向键输入。
///
/// 数据构造（含关键数值推导）：
///   ESC[?1049h = alt_screen true
///   TermMode::ALTERNATE_SCROLL 默认开启
///   未发送 ?1000h/?1002h/?1003h，因此 mouse_mode=false
///   +2 行 wheel up → 两次 Up = 2 × ESC[A；-1 行 wheel down → 一次 Down = ESC[B
///
/// 执行过程：
///   1. 只进入 alternate screen
///   2. 生成 +2 和 -1 行 wheel action
///
/// 预期结果：
///   - 正断言：+2 生成两个 Up key 序列，-1 生成一个 Down key 序列
///   - 负断言：未启用 mouse mode 时不应生成 SGR mouse bytes
#[test]
fn test_wheel_action_uses_arrow_keys_for_alt_screen_alternate_scroll() {
    let mut grid = TerminalGrid::new(80, 24);
    grid.advance(b"\x1b[?1049h");
    let snapshot = grid.snapshot();

    let up = terminal_wheel_action(&snapshot, 2, 0, 0);
    let down = terminal_wheel_action(&snapshot, -1, 0, 0);

    assert_eq!(
        up,
        TerminalWheelAction::WriteBytes(b"\x1b[A\x1b[A".to_vec()),
        "alt screen alternate scroll 的 +2 wheel 应生成两个 Up key 序列"
    );
    assert_eq!(
        down,
        TerminalWheelAction::WriteBytes(b"\x1b[B".to_vec()),
        "alt screen alternate scroll 的 -1 wheel 应生成一个 Down key 序列"
    );
    assert_ne!(
        up,
        TerminalWheelAction::WriteBytes(b"\x1b[<64;1;1M".to_vec()),
        "未启用 mouse mode 时不应生成 SGR mouse wheel bytes"
    );
}

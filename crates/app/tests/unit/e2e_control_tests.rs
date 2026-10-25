use super::protocol::{
    control_config, mouse_input, parse_request, scroll_input, text_keystrokes, Command, MousePhase,
    Response, WindowInfo,
};
use gpui::{px, MouseButton, PlatformInput, ScrollDelta};

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

/// 场景：控制通道只接受携带本次运行 token 的请求。
///
/// 数据构造：期望 token 为 32 个十六进制字符；一条带正确 token 的 mouse 请求，
/// 一条 token 只差最后一位的同样请求。
///
/// 执行过程：分别调用 `parse_request`。
///
/// 预期结果：
///   - 正断言：正确 token 解析为 `Command::Mouse`，坐标 (120, 48)。
///   - 负断言：错误 token 返回错误，不产生任何命令。
#[test]
fn request_requires_matching_token() {
    let good = format!(r#"{{"token":"{TOKEN}","op":"mouse","phase":"down","x":120,"y":48}}"#);
    let bad = good.replace("cdef\"", "cdee\"");

    let parsed = parse_request(&good, TOKEN).expect("正确 token 的请求应被接受");
    assert!(
        matches!(parsed, Command::Mouse { phase: MousePhase::Down, x, y, .. } if x == 120.0 && y == 48.0),
        "应解析为 (120, 48) 处的鼠标按下，实际 {parsed:?}"
    );
    let rejected = parse_request(&bad, TOKEN);
    assert!(rejected.is_err(), "token 不一致必须拒绝，实际 {rejected:?}");
}

/// 场景：格式错误或缺少 token 的请求被拒绝。
///
/// 数据构造：一行非 JSON 文本；一条合法 JSON 但没有 token 字段的 key 请求。
///
/// 执行过程：分别调用 `parse_request`。
///
/// 预期结果：两者都返回错误，不会退化成默认命令。
#[test]
fn malformed_or_tokenless_request_is_rejected() {
    assert!(
        parse_request("not json", TOKEN).is_err(),
        "非 JSON 请求必须拒绝"
    );
    assert!(
        parse_request(r#"{"op":"key","keystroke":"cmd-d"}"#, TOKEN).is_err(),
        "缺少 token 的请求必须拒绝"
    );
}

/// 场景：通道只有同时提供 socket 路径和足够长的 token 才启用。
///
/// 数据构造：32 字符 token（最短允许长度）；31 字符 token；缺 socket；空 socket。
///
/// 执行过程：调用 `control_config`。
///
/// 预期结果：
///   - 正断言：socket + 32 字符 token 返回配置，路径与 token 原样保留。
///   - 负断言：31 字符 token、缺 socket、空 socket 都不启用。
#[test]
fn channel_requires_socket_and_long_token() {
    let config = control_config(
        Some("/tmp/norma-e2e-control.sock".into()),
        Some(TOKEN.into()),
    )
    .expect("socket 与 32 字符 token 齐全时应启用");
    assert_eq!(
        config.socket.to_str(),
        Some("/tmp/norma-e2e-control.sock"),
        "socket 路径应原样保留"
    );
    assert_eq!(config.token, TOKEN, "token 应原样保留");

    let short = &TOKEN[..31];
    assert!(
        control_config(Some("/tmp/x.sock".into()), Some(short.into())).is_none(),
        "31 字符 token 太短，不得启用"
    );
    assert!(
        control_config(None, Some(TOKEN.into())).is_none(),
        "缺少 socket 不得启用"
    );
    assert!(
        control_config(Some(String::new()), Some(TOKEN.into())).is_none(),
        "空 socket 不得启用"
    );
}

/// 场景：鼠标命令映射为 GPUI 的窗口坐标事件。
///
/// 数据构造：在 (300, 200) 处按下，click_count=2，带 cmd 修饰；
/// 在 (310, 205) 处按住左键移动；在 (310, 205) 处抬起。
///
/// 执行过程：调用 `mouse_input` 生成 `PlatformInput`。
///
/// 预期结果：
///   - 按下：左键、位置 (300, 200)、click_count 2、platform 修饰、不是 first_mouse。
///   - 按住移动：pressed_button 为左键；未按住时为 None。
///   - 抬起：左键、click_count 1。
#[test]
fn mouse_commands_map_to_window_events() {
    let modifiers = vec!["cmd".to_string()];
    let down = mouse_input(MousePhase::Down, 300.0, 200.0, 2, false, &modifiers);
    let PlatformInput::MouseDown(down) = down else {
        panic!("down 应生成 MouseDown，实际 {down:?}");
    };
    assert_eq!(down.button, MouseButton::Left, "按下应为左键");
    assert_eq!(down.position.x, px(300.0), "按下 x 应为 300");
    assert_eq!(down.position.y, px(200.0), "按下 y 应为 200");
    assert_eq!(down.click_count, 2, "双击计数应保留为 2");
    assert!(down.modifiers.platform, "cmd 应映射为 platform 修饰");
    assert!(!down.first_mouse, "注入点击不是激活窗口的首次点击");

    let dragged = mouse_input(MousePhase::Move, 310.0, 205.0, 1, true, &[]);
    let PlatformInput::MouseMove(dragged) = dragged else {
        panic!("move 应生成 MouseMove，实际 {dragged:?}");
    };
    assert_eq!(
        dragged.pressed_button,
        Some(MouseButton::Left),
        "按住拖动时应带左键"
    );
    let hover = mouse_input(MousePhase::Move, 310.0, 205.0, 1, false, &[]);
    assert!(
        matches!(hover, PlatformInput::MouseMove(ref event) if event.pressed_button.is_none()),
        "悬停移动不应带按键，实际 {hover:?}"
    );

    let up = mouse_input(MousePhase::Up, 310.0, 205.0, 1, false, &[]);
    assert!(
        matches!(up, PlatformInput::MouseUp(ref event) if event.button == MouseButton::Left && event.click_count == 1),
        "up 应生成单击计数的左键 MouseUp，实际 {up:?}"
    );
}

/// 场景：滚轮命令使用像素增量。
///
/// 数据构造：在 (430, 360) 处 dy=-320。
///
/// 执行过程：调用 `scroll_input`。
///
/// 预期结果：生成 ScrollWheel，位置 (430, 360)，Pixels 增量 y=-320；不是 Lines 增量。
#[test]
fn scroll_command_uses_pixel_delta() {
    let event = scroll_input(430.0, 360.0, -320.0);
    let PlatformInput::ScrollWheel(event) = event else {
        panic!("应生成 ScrollWheel，实际 {event:?}");
    };
    assert_eq!(event.position.y, px(360.0), "滚动位置 y 应为 360");
    let ScrollDelta::Pixels(delta) = event.delta else {
        panic!("应为像素增量，实际 {:?}", event.delta);
    };
    assert_eq!(delta.y, px(-320.0), "像素增量 y 应为 -320");
}

/// 场景：文字按字符拆成按键，保留大小写与非 ASCII 字符。
///
/// 数据构造：文本 "Ab/抽"。
///
/// 执行过程：调用 `text_keystrokes`。
///
/// 预期结果：
///   - 4 个按键，key_char 依次为 "A" "b" "/" "抽"。
///   - "A" 的 key 为 "a" 且带 shift；"b" 不带 shift。
///   - 任何按键都不带 cmd/ctrl，避免文字被当成快捷键。
#[test]
fn text_splits_into_typed_keystrokes() {
    let keys = text_keystrokes("Ab/抽");
    let chars: Vec<_> = keys.iter().map(|key| key.key_char.as_deref()).collect();
    assert_eq!(
        chars,
        vec![Some("A"), Some("b"), Some("/"), Some("抽")],
        "每个字符都应作为 key_char 输入"
    );
    assert_eq!(keys[0].key, "a", "大写字母的 key 应为小写字母");
    assert!(keys[0].modifiers.shift, "大写字母应带 shift");
    assert!(!keys[1].modifiers.shift, "小写字母不应带 shift");
    assert!(
        keys.iter()
            .all(|key| !key.modifiers.platform && !key.modifiers.control),
        "文字输入不得带 cmd/ctrl"
    );
}

/// 场景：响应序列化为单行 JSON，失败时不附带窗口信息。
///
/// 数据构造：成功响应带窗口 7、尺寸 1280×800、缩放 2；失败响应带错误文字。
///
/// 执行过程：序列化两者。
///
/// 预期结果：
///   - 成功响应含 ok=true 与 window.number=7，且不含 error 字段、不含换行。
///   - 失败响应含 ok=false 与错误文字，且不含 window 字段。
#[test]
fn responses_serialize_as_single_lines() {
    let ok = Response::window(WindowInfo {
        number: 7,
        width: 1280.0,
        height: 800.0,
        scale: 2.0,
    })
    .to_line();
    assert!(
        ok.contains(r#""ok":true"#) && ok.contains(r#""number":7"#),
        "成功响应应含窗口编号：{ok}"
    );
    assert!(!ok.contains("error"), "成功响应不应含 error：{ok}");
    assert!(!ok.contains('\n'), "响应必须是一行：{ok}");

    let failed = Response::error("no window").to_line();
    assert!(
        failed.contains(r#""ok":false"#) && failed.contains("no window"),
        "失败响应应含错误：{failed}"
    );
    assert!(
        !failed.contains("window\":"),
        "失败响应不应含 window：{failed}"
    );
}

/// 场景：豆包转写注入请求按 token 校验并原样保留中文文本。
///
/// 数据构造：一条带正确 token 的 doubao_transcript 请求，text 为“Alan，整理 README”；
///   一条缺少 text 字段的同类请求。
///
/// 执行过程：分别调用 `parse_request`。
///
/// 预期结果：
///   - 正断言：解析为 `Command::DoubaoTranscript`，文本逐字相同。
///   - 负断言：缺少 text 的请求返回错误，不产生命令。
#[test]
fn doubao_transcript_command_carries_text() {
    let good =
        format!(r#"{{"token":"{TOKEN}","op":"doubao_transcript","text":"Alan，整理 README"}}"#);
    let parsed = parse_request(&good, TOKEN).expect("带 token 的转写注入应被接受");
    assert!(
        matches!(&parsed, Command::DoubaoTranscript { text } if text == "Alan，整理 README"),
        "应原样保留转写文本，实际 {parsed:?}"
    );
    let missing = format!(r#"{{"token":"{TOKEN}","op":"doubao_transcript"}}"#);
    assert!(
        parse_request(&missing, TOKEN).is_err(),
        "缺少 text 的请求必须拒绝"
    );
}

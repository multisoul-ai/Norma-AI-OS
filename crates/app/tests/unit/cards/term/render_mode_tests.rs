use super::*;

/// 初始化遮罩全不透明时，Terminal 卡片必须进入 IntroOnly 且不读取 live view。
///
/// 数据构造（含关键数值推导）：
///   intro.content_opacity = 0.0（前 806ms 真实内容完全不可见）
///   crossfade.content_opacity = 0.5（186ms 交叉淡入的中点）
///   initialization = None（动画结束，真实内容完全可见）
///
/// 执行过程：
///   1. 分别将 content_opacity=0.0、0.5 和 None 交给 terminal_content_mode。
///   2. 检查三种模式是否需要读取 runtime live view。
///
/// 预期结果：
///   - 正断言：0.0 是 IntroOnly，不需要 live view。
///   - 正断言：0.5 是 Crossfade，None 是 ContentOnly，两者都需要 live view。
///   - 负断言：IntroOnly 不得被判定为可构造 Terminal grid 的模式。
#[test]
fn test_terminal_content_mode_skips_live_view_while_intro_is_opaque() {
    let intro = InitializationPresentation {
        kind: crate::motion::InitializationAnimationKind::Terminal,
        frame_index: 0,
        overlay_opacity: 1.0,
        content_opacity: 0.0,
    };
    let crossfade = InitializationPresentation {
        content_opacity: 0.5,
        overlay_opacity: 0.5,
        ..intro
    };

    let intro_mode = terminal_content_mode(Some(intro));
    let crossfade_mode = terminal_content_mode(Some(crossfade));
    let content_mode = terminal_content_mode(None);

    assert_eq!(
        intro_mode,
        TerminalContentMode::IntroOnly,
        "content_opacity=0 时应只构造初始化遮罩"
    );
    assert!(
        !intro_mode.requires_live_view(),
        "IntroOnly 不得读取 Terminal snapshot 或构造 grid"
    );
    assert_eq!(
        crossfade_mode,
        TerminalContentMode::Crossfade { opacity: 0.5 },
        "content_opacity=0.5 时应同时构造遮罩和真实内容"
    );
    assert!(
        crossfade_mode.requires_live_view(),
        "Crossfade 需要真实 Terminal 内容参与淡入"
    );
    assert_eq!(
        content_mode,
        TerminalContentMode::ContentOnly,
        "无初始化遮罩时应只渲染真实内容"
    );
    assert!(
        content_mode.requires_live_view(),
        "ContentOnly 必须读取 live view 以显示 Terminal"
    );
    assert_ne!(
        intro_mode, content_mode,
        "IntroOnly 不得退化为 ContentOnly 而构造被完全遮住的 Terminal"
    );
}

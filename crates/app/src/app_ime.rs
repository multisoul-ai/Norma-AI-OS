//! IME text commit bridge for terminal-backed live nodes.

use crate::app::{AppScreen, MultisoulApp};
#[cfg(test)]
use canvas::CanvasRect;
use gpui::{
    App, Bounds, Element, ElementId, FocusHandle, GlobalElementId, InputHandler,
    InspectorElementId, IntoElement, Keystroke, LayoutId, Pixels, Point, Style, UTF16Selection,
    WeakEntity, Window,
};
use runtime::{route, InputEvent, InputMode, NodeInput, Route, Runtime};
use std::ops::Range;

pub(crate) struct TerminalImeHandler {
    app: WeakEntity<MultisoulApp>,
    marked_range: Option<Range<usize>>,
}

pub(crate) struct TerminalImeElement {
    focus_handle: FocusHandle,
    app: WeakEntity<MultisoulApp>,
}

impl TerminalImeHandler {
    pub(crate) fn new(app: WeakEntity<MultisoulApp>) -> Self {
        Self {
            app,
            marked_range: None,
        }
    }
}

impl TerminalImeElement {
    pub(crate) fn new(focus_handle: FocusHandle, app: WeakEntity<MultisoulApp>) -> Self {
        Self { focus_handle, app }
    }
}

impl IntoElement for TerminalImeElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalImeElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (window.request_layout(Style::default(), [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(
            &self.focus_handle,
            TerminalImeHandler::new(self.app.clone()),
            cx,
        );
    }
}

pub(crate) fn commit_text_to_focused_node(
    runtime: &mut Runtime,
    screen: AppScreen,
    text: &str,
) -> bool {
    if text.is_empty() || screen != AppScreen::Canvas {
        return false;
    }

    let InputMode::NodeFocused(node_id) = runtime.focus.mode() else {
        return false;
    };

    if route(&runtime.focus.mode(), &InputEvent::Key) != Route::SendInputToNode(node_id) {
        return false;
    }

    match runtime.send_node_input(node_id, NodeInput::Text(text.to_string())) {
        Ok(()) => true,
        Err(error) => {
            tracing::error!(%error, "ime commit send_node_input failed");
            false
        }
    }
}

pub(crate) fn should_defer_keystroke_to_ime(keystroke: &Keystroke) -> bool {
    if keystroke.modifiers.control || keystroke.modifiers.platform || keystroke.modifiers.function {
        return false;
    }

    if keystroke
        .key_char
        .as_deref()
        .is_some_and(|key_char| !key_char.is_empty())
    {
        return true;
    }

    keystroke.key == "space" || keystroke.key.chars().count() == 1
}

impl InputHandler for TerminalImeHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, _cx: &mut App) -> Option<Range<usize>> {
        self.marked_range.clone()
    }

    fn text_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        Some(String::new())
    }

    fn replace_text_in_range(
        &mut self,
        _replacement_range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut App,
    ) {
        self.marked_range = None;
        if let Some(app) = self.app.upgrade() {
            app.update(cx, |app, cx| {
                if commit_text_to_focused_node(&mut app.runtime, app.screen, text) {
                    cx.notify();
                }
            });
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) {
        let len_utf16 = new_text.encode_utf16().count();
        self.marked_range = Some(0..len_utf16);
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut App) {
        self.marked_range = None;
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::NodeView;
    use terminal::TerminalStatus;

    /// IME commit 文本应作为一整段 UTF-8 原文写入聚焦 terminal。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   text = "printf '中文OK'; exit\n"
    ///   ASCII 部分 "printf '' ; exit\n" 约 17 bytes，中文 "中文" = 2 chars × 3 bytes = 6 bytes
    ///   关键点不是逐键编码，而是整段 commit text 通过 NodeInput::Text 原样写入 PTY
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. Runtime 新建 /bin/sh terminal，并 focus 该 node
    ///   2. commit_text_to_focused_node(Canvas, text) → send_node_input(NodeInput::Text)
    ///   3. 循环 drain，直到 shell 执行 exit 或 3 秒超时
    ///   4. 汇总 terminal grid 文本
    ///
    /// 预期结果：
    ///   - 正断言：commit 返回 true，表示文本已投递给聚焦 node
    ///   - 正断言：屏幕包含 '中'、'文'、"OK"，证明 UTF-8 文本已进入 shell 并执行
    ///   - 负断言：屏幕不应缺少中文而只剩 OK，避免多字节文本被丢弃
    #[test]
    fn test_ime_commit_text_writes_utf8_to_focused_terminal() {
        let mut runtime = Runtime::new();
        let node_id = runtime
            .spawn_terminal(CanvasRect::new(0.0, 0.0, 640.0, 360.0), "/bin/sh", 80, 24)
            .expect("test should spawn /bin/sh terminal");
        runtime.focus.focus_node(node_id);

        let committed =
            commit_text_to_focused_node(&mut runtime, AppScreen::Canvas, "printf '中文OK'; exit\n");
        let screen_text = drain_until_terminal_exits(&mut runtime, node_id);

        assert!(committed, "IME commit text 应成功发送到当前聚焦 terminal");
        assert!(
            screen_text.contains('中'),
            "terminal 应显示命令输出中的汉字 '中'，实际屏幕: {:?}",
            screen_text
        );
        assert!(
            screen_text.contains('文'),
            "terminal 应显示命令输出中的汉字 '文'，实际屏幕: {:?}",
            screen_text
        );
        assert!(
            screen_text.contains("OK"),
            "terminal 应显示命令输出中的 ASCII 哨兵 OK，实际屏幕: {:?}",
            screen_text
        );
        assert!(
            !(screen_text.contains("OK")
                && !screen_text.contains('中')
                && !screen_text.contains('文')),
            "输出不应只剩 ASCII 'OK' 而丢失中文，实际屏幕: {:?}",
            screen_text
        );

        runtime.kill_all();
    }

    /// 非 Canvas 或未聚焦节点时，IME commit 不应写入任何 terminal。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   text = "echo SHOULD_NOT_SEND\n"，长度 21 bytes
    ///   screen = Launchpad 或 focus = CanvasSelect，二者都没有可接收文本的 node
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造空 Runtime，不创建 live node
    ///   2. Launchpad + text → commit_text_to_focused_node
    ///   3. CanvasSelect + text → commit_text_to_focused_node
    ///
    /// 预期结果：
    ///   - 正断言：两次都返回 false，表示没有写入目标
    ///   - 负断言：runtime live_len 仍为 0，没有为了输入创建隐式 terminal
    #[test]
    fn test_ime_commit_text_ignored_without_canvas_focus() {
        let mut runtime = Runtime::new();

        let from_launchpad = commit_text_to_focused_node(
            &mut runtime,
            AppScreen::Launchpad,
            "echo SHOULD_NOT_SEND\n",
        );
        let from_unfocused_canvas =
            commit_text_to_focused_node(&mut runtime, AppScreen::Canvas, "echo SHOULD_NOT_SEND\n");

        assert!(
            !from_launchpad,
            "Launchpad 上的 IME commit 不应发送到 terminal"
        );
        assert!(
            !from_unfocused_canvas,
            "CanvasSelect 未聚焦 node 时 IME commit 不应发送"
        );
        assert_eq!(
            runtime.live_len(),
            0,
            "IME commit 不应隐式创建 live terminal，live_len 应保持 0"
        );
    }

    /// 可打印字符按键应交给 IME 文本通道，控制键仍由终端按键通道处理。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   printable.key = "n"，key_char = Some("n")，无 ctrl/cmd/fn
    ///   enter.key = "enter"，key_char = None
    ///   ctrl_c.key = "c"，key_char = Some("c")，control = true
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. should_defer_keystroke_to_ime(printable) → true，让 macOS inputContext 生成 marked/commit 文本
    ///   2. should_defer_keystroke_to_ime(enter) → false，Enter 仍发给 shell
    ///   3. should_defer_keystroke_to_ime(ctrl_c) → false，Ctrl+C 仍编码为 ETX
    ///
    /// 预期结果：
    ///   - 正断言：普通 n defer，避免拼音字母先写入 PTY
    ///   - 正断言：Enter 不 defer，保持终端提交命令行为
    ///   - 负断言：Ctrl+C 不 defer，避免 SIGINT 失效
    #[test]
    fn test_printable_keystroke_deferred_to_ime_but_control_keys_are_not() {
        use gpui::{Keystroke, Modifiers};

        let printable = Keystroke {
            key: "n".into(),
            key_char: Some("n".into()),
            modifiers: Modifiers::default(),
        };
        let enter = Keystroke {
            key: "enter".into(),
            key_char: None,
            modifiers: Modifiers::default(),
        };
        let ctrl_c = Keystroke {
            key: "c".into(),
            key_char: Some("c".into()),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        };

        assert!(
            should_defer_keystroke_to_ime(&printable),
            "普通 printable 字符应 defer 给 IME，避免拼音 n 直接写入 PTY"
        );
        assert!(
            !should_defer_keystroke_to_ime(&enter),
            "Enter 不应 defer，否则终端无法提交命令"
        );
        assert!(
            !should_defer_keystroke_to_ime(&ctrl_c),
            "Ctrl+C 不应 defer，否则终端 SIGINT 会失效"
        );
    }

    fn drain_until_terminal_exits(runtime: &mut Runtime, node_id: u64) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut screen_text = String::new();
        while std::time::Instant::now() < deadline {
            runtime.drain_all();
            if let Some(NodeView::Term { snapshot, status }) = runtime.node_view(node_id) {
                screen_text = snapshot
                    .rows
                    .iter()
                    .flat_map(|row| row.iter().map(|cell| cell.c))
                    .collect();
                if matches!(status, TerminalStatus::Exited { .. }) {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        screen_text
    }
}

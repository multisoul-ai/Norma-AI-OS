use super::*;
use gpui::{KeyDownEvent, Keystroke, Modifiers, TestAppContext};
use terminal::{PtyBackend, TestPtyBackend};

fn echo_factory() -> runtime::PtyFactory {
    Box::new(|_cmd, _cols, _rows, tx| Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>))
}

/// Voice TextInput 的 marked-text 选区必须按新组合文本自身的 UTF-16 范围换算。
///
/// 场景：Model 已有 ASCII 前缀 `abc`，用户在末尾通过 macOS 中文输入法组合出 `你`。
/// 数据构造：`abc` 占 3 个 UTF-8 字节；`你` 占 3 个 UTF-8 字节、1 个 UTF-16 code unit；
/// 因此 marked range 应为 `3..6`，相对选区 `1..1` 应映射到文档 UTF-8 的 `6..6`，
/// 对外报告的文档 UTF-16 选区则是 `4..4`。
/// 执行过程：创建真实 `VoiceTextInput` entity → 同步前缀并把光标置于末尾 → 依次组合
/// `n`、`ni`、`你` → 查询 `selected_text_range` 与 `marked_text_range` → 再调用
/// `replace_text_in_range` 以 `你好` 提交组合文本。
/// 预期结果：组合期间内容、marked range 和选区均位于合法 UTF-8 边界；提交后 marked range
/// 清空、内容精确为 `abc你好`；不得把相对选区错误映射到 `你` 的中间字节 `4..4`。
#[test]
fn test_voice_text_input_maps_ime_selection_relative_to_marked_text() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = test_cx
        .add_window_view(|_window, cx| MultisoulApp::new_with_pty_factory(cx, echo_factory()));
    let input = app.read_with(cx, |app, _cx| {
        app.voice_settings_inputs
            .input(VoiceSettingsField::AgentModel)
    });

    app.update(cx, |app, cx| {
        app.commit_voice_text_input(VoiceSettingsField::AgentModel, "abc".to_string(), cx);
        app.begin_voice_text_input(VoiceSettingsField::AgentModel, "abc".to_string(), cx);
    });
    input.update(cx, |input, cx| input.sync_value("abc".to_string(), cx));
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "n", Some(1..1), window, cx);
            input.replace_and_mark_text_in_range(None, "ni", Some(2..2), window, cx);
            input.replace_and_mark_text_in_range(None, "你", Some(1..1), window, cx);
        });
    });

    let (content, marked_range, selected_range) = input.read_with(cx, |input, _cx| {
        (
            input.content.to_string(),
            input.marked_range.clone(),
            input.selected_range.clone(),
        )
    });
    assert_eq!(content, "abc你", "中文组合文本应追加到既有 ASCII 前缀后");
    assert_eq!(
        marked_range,
        Some(3..6),
        "marked range 应覆盖新插入的 `你` 三个 UTF-8 字节"
    );
    assert_eq!(
        selected_range,
        6..6,
        "相对 UTF-16 选区 1..1 应映射到新文本末尾的 UTF-8 位置"
    );
    assert_ne!(
        selected_range,
        4..4,
        "不得把相对选区按整个文档换算后落到 `你` 的中间字节"
    );
    assert!(
        content.is_char_boundary(selected_range.start)
            && content.is_char_boundary(selected_range.end),
        "IME 选区的两端都必须位于合法 UTF-8 字符边界"
    );

    let utf16_selection = cx.update(|window, cx| {
        input.update(cx, |input, cx| input.selected_text_range(false, window, cx))
    });
    let utf16_selection =
        utf16_selection.expect("平台查询时应返回当前 Voice TextInput 的 UTF-16 选区");
    assert_eq!(
        utf16_selection.range,
        4..4,
        "平台查询时应返回包含 `abc你` 四个 UTF-16 code unit 的文档选区"
    );
    assert!(!utf16_selection.reversed, "末尾光标选区不应报告为反向选择");
    let utf16_marked_range =
        cx.update(|window, cx| input.update(cx, |input, cx| input.marked_text_range(window, cx)));
    assert_eq!(
        utf16_marked_range,
        Some(3..4),
        "平台查询时 marked range 应覆盖前缀后的一个中文 UTF-16 code unit"
    );

    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "你好", window, cx);
        });
    });
    let (committed, marked_after_commit, cursor_after_commit) =
        input.read_with(cx, |input, _cx| {
            (
                input.content.to_string(),
                input.marked_range.clone(),
                input.selected_range.clone(),
            )
        });
    assert_eq!(
        committed, "abc你好",
        "提交文本应替换旧 marked text 并保留既有前缀"
    );
    assert_eq!(
        marked_after_commit, None,
        "IME 提交完成后不应残留 marked range"
    );
    assert_eq!(
        cursor_after_commit,
        9..9,
        "abc(3 字节)+你好(各 3 字节) 后光标应位于第 9 字节"
    );
    let saved = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        saved.agent_model, "abc你好",
        "IME replace_text 最终提交路径应自动保存完整中文文本，snapshot: {:?}",
        saved
    );
}

/// 中文输入法组合期间只更新草稿，结束组合后才自动保存最终文本。
///
/// 场景：Model 的已保存值为 `abc`，用户在末尾通过 IME 依次组合 `n`、`ni`、`你`，
/// 最后由平台调用 `unmark_text` 接受当前 marked text。
/// 数据构造：正式值固定为 `abc`；组合后的可见内容为 `abc你`，二者可明确区分草稿与
/// 已保存设置；全程不发送 Enter。
/// 执行过程：先通过 App 提交基线并重新进入编辑态，再驱动真实 EntityInputHandler 的
/// `replace_and_mark_text_in_range`；组合期间读取 Settings snapshot，再将 Enter/Escape
/// 回派给真实 key handler，随后调用 unmark 并再次读取 snapshot。
/// 预期结果：组合期间正式 Model 仍为 `abc`、不得出现 `abc你`；unmark 后正式 Model
/// 精确变为 `abc你`，且 Model 仍处于编辑态；marked 状态下 Enter/Escape 不得提交或取消
/// 拼音半成品，证明自动保存没有伪装成 Enter 提交。
#[test]
fn test_voice_text_input_auto_saves_only_after_ime_unmark() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = test_cx
        .add_window_view(|_window, cx| MultisoulApp::new_with_pty_factory(cx, echo_factory()));
    let input = app.read_with(cx, |app, _cx| {
        app.voice_settings_inputs
            .input(VoiceSettingsField::AgentModel)
    });

    app.update(cx, |app, cx| {
        app.commit_voice_text_input(VoiceSettingsField::AgentModel, "abc".to_string(), cx);
        app.begin_voice_text_input(VoiceSettingsField::AgentModel, "abc".to_string(), cx);
    });
    input.update(cx, |input, cx| input.sync_value("abc".to_string(), cx));
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "n", Some(1..1), window, cx);
            input.replace_and_mark_text_in_range(None, "ni", Some(2..2), window, cx);
            input.replace_and_mark_text_in_range(None, "你", Some(1..1), window, cx);
        });
    });

    let composing = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        composing.agent_model, "abc",
        "IME marked text 组合期间不得覆盖正式 Model，snapshot: {:?}",
        composing
    );
    assert_ne!(
        composing.agent_model, "abc你",
        "IME 组合半成品不得提前自动保存，snapshot: {:?}",
        composing
    );

    for key in ["enter", "escape"] {
        let event = KeyDownEvent {
            keystroke: Keystroke {
                modifiers: Modifiers::default(),
                key: key.to_string(),
                key_char: None,
            },
            is_held: false,
        };
        cx.update(|window, cx| {
            input.update(cx, |input, cx| input.on_key_down(&event, window, cx));
        });
    }
    let guarded = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        guarded.agent_model, "abc",
        "marked 状态下 Enter/Escape 不得保存中文组合半成品"
    );
    assert_eq!(
        input.read_with(cx, |input, _cx| input.marked_range.clone()),
        Some(3..6),
        "marked 状态下 Enter/Escape 应继续交由输入法处理，不能清除组合范围"
    );

    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.unmark_text(window, cx));
    });
    let saved = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        saved.agent_model, "abc你",
        "IME unmark 后应自动保存最终组合文本，snapshot: {:?}",
        saved
    );
    assert_eq!(
        saved.voice_editing_field,
        Some("agent_model"),
        "自动保存后 Model 应保持编辑态而非模拟 Enter，snapshot: {:?}",
        saved
    );
}

/// 已配置 API Key 的空白安全输入框，未修改或取消 IME 组合后按 Enter 不得删除密钥。
///
/// 场景：SettingsStore 已有 API Key；出于安全考虑输入实体不回填密钥明文，用户只聚焦
/// 空白输入框并按 Enter；随后再次聚焦，开始组合 `n` 后取消为原空内容，再按 Enter。
/// 数据构造：临时 SQLite 先通过 VoiceSettingsState 写入测试密钥；断言仅使用 configured
/// 布尔值，失败消息和快照都不暴露密钥明文。
/// 执行过程：从临时 store 构造 App，先走一次未修改 submit；再进入编辑态，通过真实
/// marked-text handler 执行 `n → 空` 的组合取消并 submit，最后重新加载 Provider 配置。
/// 预期结果：两次操作后当前 App 与重新加载结果都保持 API Key configured；不得因安全
/// 输入框初始为空，或取消组合曾经短暂改变内容，而隐式执行 Delete。
#[test]
fn test_voice_text_input_untouched_api_key_enter_preserves_saved_secret() {
    let data = tempfile::TempDir::new().expect("测试应创建临时 settings 目录");
    let store = storage::SettingsStore::new(data.path().join("app.sqlite"));
    let mut initial = VoiceSettingsState::load_from_store(store.clone());
    initial.begin_edit(VoiceSettingsField::AgentApiKey);
    let accepted =
        initial.commit_edit_value(VoiceSettingsField::AgentApiKey, "test-api-key".to_string());
    assert!(accepted.is_some(), "测试前置 API Key 应被状态层接受");
    let writes = initial.take_pending_writes();
    assert_eq!(writes.len(), 1, "测试前置 API Key 应产生一个持久化写入");
    for write in writes {
        write.execute().expect("测试前置 API Key 写入应成功");
    }
    initial.record_persistence_result(Ok(()));

    let mut test_cx = TestAppContext::single();
    let (app, cx) = test_cx.add_window_view(|_window, cx| {
        MultisoulApp::new_with_pty_factory_and_voice_settings_store(
            cx,
            echo_factory(),
            store.clone(),
        )
    });
    let input = app.read_with(cx, |app, _cx| {
        app.voice_settings_inputs
            .input(VoiceSettingsField::AgentApiKey)
    });
    app.update(cx, |app, cx| {
        app.begin_voice_text_input(VoiceSettingsField::AgentApiKey, String::new(), cx)
    });
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.focus_handle.focus(window);
            input.submit(window, cx);
        });
    });
    cx.run_until_parked();

    let current = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        current.agent_api_key_configured,
        "未修改 API Key 直接按 Enter 后当前配置仍应为 configured"
    );
    assert_eq!(
        current.voice_editing_field, None,
        "未修改 API Key 按 Enter 后应正常退出编辑态"
    );

    app.update(cx, |app, cx| {
        app.begin_voice_text_input(VoiceSettingsField::AgentApiKey, String::new(), cx)
    });
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.focus_handle.focus(window);
            input.replace_and_mark_text_in_range(None, "n", Some(1..1), window, cx);
            input.replace_and_mark_text_in_range(None, "", Some(0..0), window, cx);
            input.submit(window, cx);
        });
    });
    cx.run_until_parked();
    let after_cancel = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        after_cancel.agent_api_key_configured,
        "取消 API Key IME 组合后按 Enter 仍应保持 configured"
    );
    assert_eq!(
        after_cancel.voice_editing_field, None,
        "取消 API Key IME 组合后按 Enter 应正常退出编辑态"
    );
    let (_base_url, _model, configured) = MultisoulApp::load_voice_provider_settings_for_e2e(store);
    assert!(
        configured,
        "重新加载后 API Key 仍应为 configured，不能被隐式删除"
    );
}

/// API Key 自动保存后按 Escape，输入实体必须清除明文并显示固定 configured 状态。
///
/// 场景：用户在安全输入框输入新 API Key，自动保存已接受该值，随后按 Escape 离开；
/// Escape 不能把保存基线中的密钥重新留在可复制的 `content` / `original_content`。
/// 数据构造：使用 in-memory Voice state 与真实 VoiceTextInput entity；测试密钥只传给
/// 平台文本替换入口，断言只检查布尔状态，失败输出不包含输入内容。
/// 执行过程：进入 API Key 编辑态、聚焦并调用真实 `replace_text_in_range`，确认自动保存
/// 已使 Provider configured，再调用真实 cancel/Escape 路径并检查实体安全状态。
/// 预期结果：正式配置保持 configured；content 与 original_content 均为空；placeholder
/// 为固定 configured 提示，API Key 明文不得在失焦后的输入实体中继续存活。
#[test]
fn test_voice_text_input_escape_clears_api_key_entity_after_autosave() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = test_cx
        .add_window_view(|_window, cx| MultisoulApp::new_with_pty_factory(cx, echo_factory()));
    let input = app.read_with(cx, |app, _cx| {
        app.voice_settings_inputs
            .input(VoiceSettingsField::AgentApiKey)
    });
    app.update(cx, |app, cx| {
        app.begin_voice_text_input(VoiceSettingsField::AgentApiKey, String::new(), cx)
    });
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.focus_handle.focus(window);
            input.replace_text_in_range(None, "escape-test-key", window, cx);
        });
    });
    let before_escape = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        before_escape.agent_api_key_configured,
        "Escape 前自动保存应已把当前 Provider 标记为 configured"
    );
    let (holds_editing_content, baseline_already_cleared) = input.read_with(cx, |input, _cx| {
        (!input.content.is_empty(), input.original_content.is_empty())
    });
    assert!(
        holds_editing_content,
        "测试前置应确认聚焦编辑内容存在，避免 Escape 清理断言假绿"
    );
    assert!(
        baseline_already_cleared,
        "API Key 自动保存后不得把明文复制到 original_content"
    );

    cx.update(|window, cx| input.update(cx, |input, cx| input.cancel(window, cx)));
    cx.run_until_parked();

    let (content_cleared, original_cleared, configured_placeholder) =
        input.read_with(cx, |input, _cx| {
            (
                input.content.is_empty(),
                input.original_content.is_empty(),
                input.placeholder == status::CONFIGURED_API_KEY_PLACEHOLDER,
            )
        });
    assert!(content_cleared, "Escape 后 API Key content 必须被清空");
    assert!(
        original_cleared,
        "Escape 后 API Key original_content 必须被清空"
    );
    assert!(
        configured_placeholder,
        "Escape 后 API Key 应显示固定 configured 提示"
    );
    let after_escape = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        after_escape.agent_api_key_configured,
        "Escape 只结束编辑，不得删除已自动保存的 API Key"
    );
}

/// 程序化关闭 Settings 必须主动收口 API Key 编辑态，不能依赖鼠标先制造 blur。
///
/// 场景：菜单 action/快捷键直接调用 `toggle_settings_panel` 关闭面板，此时没有关闭按钮
/// 的 mouse-down 焦点变化；已自动保存的 API Key 仍在输入实体的聚焦内容中。
/// 数据构造：in-memory App、真实 API Key 输入实体和测试输入；断言仅检查安全布尔状态。
/// 执行过程：打开面板状态、进入 API Key 编辑并走平台文本替换，然后直接调用 toggle。
/// 预期结果：面板关闭、编辑态结束；content/original_content 为空且提示为 configured。
#[test]
fn test_settings_programmatic_close_clears_api_key_input_entity() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = test_cx
        .add_window_view(|_window, cx| MultisoulApp::new_with_pty_factory(cx, echo_factory()));
    let input = app.read_with(cx, |app, _cx| {
        app.voice_settings_inputs
            .input(VoiceSettingsField::AgentApiKey)
    });
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx);
        app.settings_panel.open();
        app.begin_voice_text_input(VoiceSettingsField::AgentApiKey, String::new(), cx);
    });
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "programmatic-close-test-key", window, cx)
        });
    });

    app.update(cx, |app, cx| app.toggle_settings_panel(cx));

    let closed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(!closed.settings_open, "程序化 toggle 后 Settings 应关闭");
    assert_eq!(
        closed.voice_editing_field, None,
        "程序化关闭后 API Key 编辑态必须结束"
    );
    let secure = input.read_with(cx, |input, _cx| {
        input.content.is_empty()
            && input.original_content.is_empty()
            && input.placeholder == status::CONFIGURED_API_KEY_PLACEHOLDER
    });
    assert!(
        secure,
        "程序化关闭后 API Key 输入实体必须清除明文并显示 configured"
    );
}

/// 必填 Model 被清空后按 Enter，应拒绝提交并保持输入焦点与有效恢复基线。
///
/// 场景：用户全选当前 Model、删除全部字符并立即按 Enter。
/// 数据构造：基线从 App 当前正式 Model 读取；输入实体先同步同一基线，再通过真实
/// `replace_text_in_range` 删除完整 UTF-8 范围，避免直接篡改 state。
/// 执行过程：聚焦输入实体、全选、替换为空，再调用 submit；随后检查 App snapshot、
/// 输入焦点、当前可见内容和 original_content。
/// 预期结果：正式 Model 与 original_content 都保持原有效值，输入内容可以暂时为空且
/// 继续聚焦、编辑态继续存在；不得无条件失焦或把无效空值设成 Escape 恢复基线。
#[test]
fn test_voice_text_input_empty_required_submit_keeps_focus_and_valid_baseline() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = test_cx
        .add_window_view(|_window, cx| MultisoulApp::new_with_pty_factory(cx, echo_factory()));
    let baseline = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx).agent_model);
    let input = app.read_with(cx, |app, _cx| {
        app.voice_settings_inputs
            .input(VoiceSettingsField::AgentModel)
    });
    app.update(cx, |app, cx| {
        app.begin_voice_text_input(VoiceSettingsField::AgentModel, baseline.clone(), cx)
    });
    input.update(cx, |input, cx| input.sync_value(baseline.clone(), cx));

    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.focus_handle.focus(window);
            input.selected_range = 0..input.content.len();
            input.replace_text_in_range(None, "", window, cx);
            input.submit(window, cx);
        });
    });

    let after = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        after.agent_model, baseline,
        "空 Model 提交被拒绝后正式值应保持输入前基线，snapshot: {:?}",
        after
    );
    assert_eq!(
        after.voice_editing_field,
        Some("agent_model"),
        "空 Model 提交被拒绝后应继续处于 Model 编辑态"
    );
    let (content, original_content) = input.read_with(cx, |input, _cx| {
        (
            input.content.to_string(),
            input.original_content.to_string(),
        )
    });
    assert_eq!(content, "", "被拒绝后输入框可保持暂时空白供用户继续输入");
    assert_eq!(
        original_content, baseline,
        "被拒绝后 Escape 恢复基线必须仍是原有效 Model"
    );
    let focused = cx.update(|window, cx| input.read(cx).focus_handle.is_focused(window));
    assert!(focused, "空 Model 提交被拒绝后真实输入焦点必须保留");
}

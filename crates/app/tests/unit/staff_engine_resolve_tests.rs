use super::*;
use gpui::TestAppContext;

fn definition(
    id: i64,
    name: &str,
    profile: canvas::AgentProfile,
    enabled: bool,
) -> storage::AgentDefinition {
    storage::AgentDefinition {
        id,
        name: name.into(),
        speech_alias: None,
        runtime_profile: profile,
        workflow_description: String::new(),
        enabled,
        sort_order: id,
        origin: storage::AgentDefinitionOrigin::Explicit,
    }
}

/// [场景描述]：幕僚用 engine 标识派发时，宿主只在已启用的同类型里选，并优先名册里更靠前的空闲项。
///
/// 数据构造：
///   id=1 Chloe / Claude / 停用
///   id=2 Bella / Claude / 启用
///   id=3 Zoe  / Claude / 启用
///   id=4 Alan / Codex  / 启用
///   四条都没有进行中的卡片，busy 全是 false，排序保持名册顺序。
///
/// 执行过程：
///   1. resolve("claude")：停用的 Chloe 被滤掉，Bella、Zoe 都匹配，取名册里更靠前的 Bella
///   2. resolve("CODEX")：大小写不同，仍命中 Alan
///   3. resolve("Bella")：旧的具名智能体名仍命中
///   4. resolve("Chloe")：名字对上但已停用，不能因为还有别的 Claude 就选中她
///
/// 预期结果：
///   - "claude" 选中 Bella（id=2），不是停用的 Chloe，也不是更靠后的 Zoe
///   - "CODEX" 与 "Bella" 分别选中 Alan、Bella
///   - "Chloe" 失败，错误里列出 claude 和 codex，不把停用类型算进可选项
#[test]
fn resolve_staff_entry_picks_an_enabled_engine_and_keeps_old_names() {
    let mut test = TestAppContext::single();
    let (session, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    session.update(cx, |app, _| {
        app.agent_definitions.replace_definitions_for_test(vec![
            definition(1, "Chloe", canvas::AgentProfile::Claude, false),
            definition(2, "Bella", canvas::AgentProfile::Claude, true),
            definition(3, "Zoe", canvas::AgentProfile::Claude, true),
            definition(4, "Alan", canvas::AgentProfile::Codex, true),
        ]);
        let claude = app
            .resolve_staff_entry("claude")
            .expect("enabled claude must resolve");
        assert_eq!(
            claude.definition_id, 2,
            "停用的 Chloe 不能被选中，应取名册里第一个启用的 Claude"
        );
        assert_eq!(
            claude.name, "Bella",
            "选中的卡片名来自定义，不是幕僚传入的 engine"
        );
        let codex = app
            .resolve_staff_entry("CODEX")
            .expect("engine match is case-insensitive");
        assert_eq!(codex.name, "Alan", "CODEX 必须落到 Codex 类型的 Alan");
        let named = app
            .resolve_staff_entry("Bella")
            .expect("legacy agent name must still resolve");
        assert_eq!(named.definition_id, 2, "旧的具名智能体名仍指向 Bella");
        let disabled = app
            .resolve_staff_entry("Chloe")
            .expect_err("disabled name must not resolve");
        let (code, listed) = disabled
            .split_once('\n')
            .expect("unavailable error keeps a code and the engine list");
        assert_eq!(code, "staff.agent_unavailable", "展示边界只认这个错误码");
        assert_eq!(
            listed, "claude, codex",
            "可选项只含已启用类型，且按类型名排序"
        );
        assert_eq!(
            crate::app::staff::error_message(disabled),
            ui::i18n::message(
                "staff.agent_unavailable_choices",
                &[("engines", "claude, codex".into())],
            ),
            "用户和幕僚必须看到已翻译的说明以及可选类型，不能落到笼统的操作失败"
        );
    });
}

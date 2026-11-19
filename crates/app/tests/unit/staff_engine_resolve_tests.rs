use super::*;
use gpui::TestAppContext;
use norma::orchestration::{ComponentState, OrchestrationProvider, ProviderIntegration};
use norma::protocol::AgentProfile;

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

fn ready(provider: OrchestrationProvider) -> ProviderIntegration {
    let component = ComponentState::Ready {
        version: "1".into(),
    };
    ProviderIntegration::from_components(
        provider,
        component.clone(),
        component.clone(),
        component.clone(),
        component,
    )
}

fn install_failed(provider: OrchestrationProvider) -> ProviderIntegration {
    ProviderIntegration::from_components(
        provider,
        ComponentState::Failed {
            error_code: "install_failed".into(),
            message: "cli missing".into(),
        },
        ComponentState::Missing,
        ComponentState::Missing,
        ComponentState::Missing,
    )
}

/// [场景描述]：幕僚用 engine 标识派发时，宿主只在已启用的同类型里选，并优先名册里更靠前的空闲项。
/// 可派类型来自编排里注册成功的引擎，不是名册字母序。
///
/// 数据构造：
///   编排就绪 = Codex、Claude（OrchestrationProvider::ALL 里 Codex 在 Claude 前）
///   id=1 Chloe / Claude / 停用
///   id=2 Bella / Claude / 启用
///   id=3 Zoe  / Claude / 启用
///   id=4 Alan / Codex  / 启用
///   四条都没有进行中的卡片，busy 全是 false，排序保持名册顺序。
///
/// 执行过程：
///   1. resolve("claude")：停用的 Chloe 被滤掉，Bella、Zoe 都匹配，取名册里更靠前的 Bella
///   2. resolve("CODEX")：大小写不同，仍命中 Alan
///   3. resolve("Bella")：旧的具名智能体名仍命中，且其引擎已注册
///   4. resolve("Chloe")：名字对上但已停用，不能因为还有别的 Claude 就选中她
///
/// 预期结果：
///   - "claude" 选中 Bella（id=2），显示名是编排里的 "Claude Code"，不是 Debug 名 "Claude"
///   - "CODEX" 与 "Bella" 分别选中 Alan、Bella
///   - "Chloe" 失败，错误里按编排顺序列出 codex, claude，不把停用类型算进可选项
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
        app.orchestration_settings.set_integrations_for_test(vec![
            ready(OrchestrationProvider::Codex),
            ready(OrchestrationProvider::Claude),
        ]);
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
        assert_eq!(
            app.staff_engine_label("claude"),
            "Claude Code",
            "建议指派必须显示编排里的引擎名，不能显示 Debug 名 Claude"
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
            listed, "codex, claude",
            "可选项按编排页顺序，只含注册成功的引擎"
        );
        assert_eq!(
            crate::app::staff::error_message(disabled),
            ui::i18n::message(
                "staff.agent_unavailable_choices",
                &[("engines", "codex, claude".into())],
            ),
            "用户和幕僚必须看到已翻译的说明以及可选类型，不能落到笼统的操作失败"
        );
    });
}

/// [场景描述]：建议指派的可选类型只看编排里注册成功的引擎，不看画布卡片，也不看名册名字。
///
/// 数据构造：
///   编排就绪 = Codex、Claude、Grok（ALL 顺序：Codex、Claude、Cursor、OpenCode、InfCode、Grok）
///   Cursor 安装失败，Hermes 未写入编排（视为未安装）
///   名册：
///     id=1 名字 "Codex" / 实际类型 Claude / 启用（名字和类型故意对不上）
///     id=2 CursorCard / Cursor / 启用
///     id=3 Alan / Codex / 启用
///   画布：一张 Hermes 卡片，没有对应名册，也没有注册
///   Grok 注册成功，但名册和画布都没有 Grok
///
/// 执行过程：
///   1. 读取建议指派的可选类型
///   2. 用 "codex" 解析类型和执行者
///   3. 用 "grok"、"cursor"、"CursorCard"、"hermes" 分别解析
///
/// 预期结果：
///   - 可选项正好是 codex、claude、grok，顺序同编排页
///   - 不包含 cursor（注册失败）和 hermes（画布上有卡片但未注册）
///   - "codex" 的类型是 Codex、执行者是 Alan，不能因为有一张名叫 Codex 的 Claude 名册就显示成 Claude
///   - "grok" 没有卡片也能解析，显示名是 "Grok Build"
///   - "cursor" / "CursorCard" / "hermes" 都不可派
#[test]
fn staff_assignable_types_follow_registered_engines_not_canvas_cards() {
    let mut test = TestAppContext::single();
    let (session, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    session.update(cx, |app, _| {
        app.orchestration_settings.set_integrations_for_test(vec![
            ready(OrchestrationProvider::Codex),
            ready(OrchestrationProvider::Claude),
            ready(OrchestrationProvider::Grok),
            install_failed(OrchestrationProvider::Cursor),
        ]);
        app.agent_definitions.replace_definitions_for_test(vec![
            definition(1, "Codex", canvas::AgentProfile::Claude, true),
            definition(2, "CursorCard", canvas::AgentProfile::Cursor, true),
            definition(3, "Alan", canvas::AgentProfile::Codex, true),
        ]);
        app.runtime.document.add_node(
            canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
                profile: canvas::AgentProfile::Hermes,
                alias: Some("Hermes on canvas".into()),
                definition_id: None,
                mobile_control: false,
            }),
            canvas::CanvasRect::new(0.0, 0.0, 320.0, 240.0),
        );

        let choices = app.staff_agent_choices();
        assert_eq!(
            choices,
            vec![
                ("codex".into(), AgentProfile::Codex),
                ("claude".into(), AgentProfile::Claude),
                ("grok".into(), AgentProfile::Grok),
            ],
            "可选类型必须是注册成功的引擎，顺序同编排页，不能跟着画布或名册走"
        );
        assert!(
            !choices
                .iter()
                .any(|(id, _)| id == "cursor" || id == "hermes"),
            "注册失败的 Cursor 和只有画布卡片的 Hermes 不能出现在可派类型里"
        );
        assert_eq!(
            app.staff_engine_profile("codex"),
            Some(AgentProfile::Codex),
            "名叫 Codex 的 Claude 名册不能把 codex 推断成 Claude"
        );
        assert_eq!(
            app.staff_engine_label("codex"),
            "Codex",
            "codex 的显示名来自编排，不是名册名字"
        );
        assert_eq!(
            app.staff_engine_label("claude"),
            "Claude Code",
            "claude 必须显示编排名 Claude Code"
        );
        assert_eq!(
            app.staff_engine_label("grok"),
            "Grok Build",
            "没有卡片时，注册成功的 Grok 仍要显示 Grok Build"
        );
        assert_eq!(
            app.staff_engine_label("cursor"),
            "cursor",
            "注册失败的引擎不能借卡片显示成 Cursor Agent CLI"
        );
        assert_eq!(
            app.staff_engine_profile("cursor"),
            None,
            "注册失败的 Cursor 不是可派类型"
        );
        assert_eq!(
            app.staff_engine_profile("hermes"),
            None,
            "画布上的 Hermes 卡片不能变成可派类型"
        );

        let codex = app
            .resolve_staff_entry("codex")
            .expect("registered codex must resolve");
        assert_eq!(
            codex.definition_id, 3,
            "codex 必须落到真正的 Codex 名册 Alan"
        );
        assert_eq!(codex.name, "Alan", "不能选中名叫 Codex 的 Claude 名册");
        assert_eq!(
            codex.profile,
            AgentProfile::Codex,
            "codex 的类型必须是 Codex"
        );
        let grok = app
            .resolve_staff_entry("grok")
            .expect("registered grok must resolve without a canvas card");
        assert_eq!(grok.name, "Grok Build", "没有名册时用编排显示名");
        assert_eq!(grok.profile, AgentProfile::Grok, "grok 的类型必须是 Grok");
        assert_eq!(grok.definition_id, 0, "没有名册时不应绑到一张已有卡片");
        assert!(
            grok.active_node_id.is_none(),
            "没有画布卡片时不能假装已有节点"
        );

        for id in ["cursor", "CursorCard", "hermes"] {
            let error = app
                .resolve_staff_entry(id)
                .expect_err("unregistered or failed engines are not assignable");
            let (code, listed) = error
                .split_once('\n')
                .expect("unavailable error keeps a code and the engine list");
            assert_eq!(code, "staff.agent_unavailable", "{id} 必须被拒绝");
            assert_eq!(
                listed, "codex, claude, grok",
                "{id} 的可选项不能带上 cursor 或 hermes"
            );
            assert!(
                !listed
                    .split(", ")
                    .any(|engine| engine == "cursor" || engine == "hermes"),
                "{id} 的错误列表泄漏了未注册引擎：{listed}"
            );
        }
    });
}

/// 场景：语音别名能选中名册里的人；引擎 id 即使原话里写了别人的名字也不算点名。
///
/// 数据构造：Bella / Claude / 启用，语音别名「小王」。编排里 Claude 已注册。
/// 执行过程：用别名解析执行者，再用原话判断是否点名。
/// 预期结果：
///   - 正断言：「小王」解析到 Bella；「让小王删一下」算点名
///   - 负断言：「claude」仍走引擎挑选且不算点名；「用 claude 做」对 Bella 也不算点名
#[test]
fn speech_alias_selects_the_person_but_engine_ids_are_unnamed() {
    let mut test = TestAppContext::single();
    let (session, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    session.update(cx, |app, _| {
        app.orchestration_settings
            .set_integrations_for_test(vec![ready(OrchestrationProvider::Claude)]);
        let mut bella = definition(2, "Bella", canvas::AgentProfile::Claude, true);
        bella.speech_alias = Some("小王".into());
        app.agent_definitions
            .replace_definitions_for_test(vec![bella]);
        let picked = app
            .resolve_staff_entry("小王")
            .expect("speech alias must resolve");
        assert_eq!(
            picked.definition_id, 2,
            "别名必须落到 Bella，不能另开引擎卡"
        );
        assert_eq!(picked.name, "Bella", "选中的仍是名册名");
        let named = |app: &MultisoulApp, id: &str, words: &str| {
            app.roster_entry_if_named_target(id).is_some_and(|entry| {
                brain::staff::naming::named_in(
                    words,
                    brain::staff::naming::NameRef {
                        name: &entry.name,
                        speech_alias: entry.speech_alias.as_deref(),
                    },
                )
            })
        };
        assert!(named(app, "小王", "让小王删一下"), "原话里的别名算点名");
        assert!(named(app, "Bella", "让 Bella 合并"), "原话里的名册名算点名");
        let engine = app
            .resolve_staff_entry("claude")
            .expect("engine id still resolves");
        assert_eq!(engine.definition_id, 2, "引擎 id 仍挑这张 Claude 卡");
        assert!(
            app.roster_entry_if_named_target("claude").is_none(),
            "引擎 id 按没点名处理，即使原话写了人名"
        );
        assert!(
            !named(app, "Bella", "用 claude 做"),
            "只说引擎名不算点了 Bella"
        );
    });
}

use super::super::session::{CallEnd, CallEvent};
use super::*;
use live_voice::{LiveUsage, Speaker, TranscriptFragment};

/// AC06/AC11：停止播报独立于输入静音、通话和已接收任务，并且恢复不恢复旧音频。
/// 构造：活跃已连接通话、可观察命令通道；先停止再收到确认，然后恢复。
/// 执行：请求输出阻断，核对命令与View；确认后恢复输出，不触发stop。
/// 预期：输出开关与麦克风静音互不影响；通话token始终有效。
#[test]
fn gpt_live_output_control_is_independent_of_microphone_and_call_lifetime() {
    let mut state = GptLiveVoiceState::default();
    let (generation, cancellation) = state.begin();
    state.apply(generation, CallEvent::Connected);
    let (commands, received) = async_channel::bounded(8);
    state.commands = Some(commands);
    state.request_output().expect("停止播报");
    assert!(
        matches!(received.try_recv(), Ok(CallCommand::SetOutputBlocked(true))),
        "独立输出命令"
    );
    state.apply(generation, CallEvent::OutputBlocked(true));
    assert!(state.view().output_blocked, "停止状态清晰可见");
    assert!(
        !state.is_muted() && !cancellation.is_cancelled(),
        "停止播报不能静音或挂断"
    );
    state.request_output().expect("恢复播报");
    assert!(
        matches!(
            received.try_recv(),
            Ok(CallCommand::SetOutputBlocked(false))
        ),
        "恢复只接收新音频"
    );
    assert!(state.is_active(), "任务和通话继续有效");
}

fn caption(id: &str, speaker: Speaker, text: &str) -> CallEvent {
    CallEvent::Transcript(TranscriptFragment {
        event_id: id.into(),
        speaker,
        delta: text.into(),
        start_ms: 0.0,
        end_ms: 100.0,
    })
}

/// AC04/AC11/AC13：通话停止立即失效旧generation，双方字幕独立增长，迟到终态只补该通话用量。
/// 数据构造：首通话交错user/assistant字幕并停止，晚到final usage；第二通话收到旧回调。
/// 执行过程：begin→apply双方字幕→stop→迟到回调→begin新通话→重放旧回调→本轮静音。
/// 预期结果：旧音频/字幕不能复活；用量可在挂断后确认，但不覆盖新通话；媒体token取消。
#[test]
fn call_state_isolates_generations_captions_and_final_usage() {
    let mut state = GptLiveVoiceState::default();
    let (first, media) = state.begin();
    assert!(state.apply(first, CallEvent::Connected), "首通话连接");
    state.apply(first, caption("u1", Speaker::User, "创建一个"));
    state.apply(first, caption("a1", Speaker::Assistant, "我在听"));
    state.apply(first, caption("u2", Speaker::User, "终端"));
    let text = state.caption();
    assert!(text.contains("创建一个终端"), "用户字幕跨助手附和连续增长");
    assert!(text.contains("我在听"), "助手字幕同时保留");
    state.stop();
    assert!(media.is_cancelled(), "停止只取消本通话媒体token");
    assert!(!state.is_active(), "停止立即不再活跃");
    assert!(
        !state.apply(first, caption("late", Speaker::Assistant, "旧回复")),
        "旧generation不可更新字幕"
    );
    assert!(
        !state.finish(
            first,
            CallEnd {
                usage: LiveUsage {
                    seconds: 3.5,
                    finalized: true
                },
                error: None
            }
        ),
        "迟到终态不能复活通话"
    );
    assert_eq!(state.usage().seconds, 3.5, "挂断后仍记录该通话最终用量");
    assert!(state.usage().finalized, "挂断后可补充closed确认");
    let (second, _) = state.begin();
    assert!(
        !state.apply(first, CallEvent::Connected),
        "新通话不接受旧连接事件"
    );
    state.finish(
        first,
        CallEnd {
            usage: LiveUsage {
                seconds: 99.0,
                finalized: true,
            },
            error: Some("old failure".into()),
        },
    );
    assert_eq!(state.usage().seconds, 0.0, "旧用量不能覆盖新通话");
    assert!(!state.caption().contains("旧回复"), "新通话不残留旧字幕");
    state.apply(second, CallEvent::Connected);
    state.apply(second, CallEvent::Muted(true));
    assert!(state.is_muted(), "本轮静音状态仍可更新");
}

/// AC11/AC13：跨窗口模式、麦克风或Live凭据变化必须取消旧通话，音色仅影响下次通话。
/// 数据构造：每种变化各一个隔离SQLite库与TestAppContext；真实取消令牌，无麦克风/网络。
/// 执行过程：保存GPT模式→接收者开始通话→另一窗口落库变更→生产resync→检查媒体取消。
/// 预期结果：模式/设备/Key引用变更都停止；音色改变不打断；这不是实际窗口或硬件验收。
#[test]
fn gpt_live_remote_configuration_change_stops_only_affected_media() {
    use crate::{app::MultisoulApp, settings::voice::VoiceSettingsState};
    use gpui::TestAppContext;
    for (key, value, should_stop) in [
        ("voice.desktop.mode", "legacy", true),
        ("voice.input_device_id", "fixture-device", true),
        (
            "voice.gpt_live.credential_ref",
            "gpt-live-desktop-changed",
            true,
        ),
        ("voice.gpt_live.voice", "cedar", false),
    ] {
        let temp = tempfile::tempdir().expect("隔离目录");
        let store = storage::SettingsStore::new(temp.path().join("settings.sqlite"));
        store
            .set_string("voice.desktop.mode", "gpt_live")
            .expect("初始GPT模式");
        let mut test = TestAppContext::single();
        let (app, cx) = test.add_window_view(|_, cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            )
        });
        let media = app.update(cx, |app, _| {
            app.voice_settings = VoiceSettingsState::load_from_store(store.clone());
            let (_, media) = app.gpt_live.begin();
            media
        });
        store.set_string(key, value).expect("外部窗口保存");
        app.update(cx, |app, cx| app.resync_voice_settings(false, cx));
        cx.run_until_parked();
        assert_eq!(
            media.is_cancelled(),
            should_stop,
            "{key}变化的通话取消必须符合配置语义"
        );
        app.read_with(cx, |app, _| {
            assert_eq!(app.gpt_live.is_active(), !should_stop, "{key}后的活跃状态");
        });
    }
}

/// AC12/AC13：选择同一模式不挂断，切换模式立即取消本通话并保留可恢复配置。
/// 数据构造：隔离App状态、GPT模式与媒体token，无真实设备；连接后模拟静音操作。
/// 执行过程：重复选择GPT→请求静音并接收command→本地gate确认→切到Legacy→核对只读快照。
/// 预期结果：重复选择保持通话；pending静音不可重复；切换后inactive，最后用量仍可查询。
#[test]
fn gpt_live_mode_selection_and_view_preserve_call_control_semantics() {
    use crate::{app::MultisoulApp, settings::voice::DesktopVoiceMode};
    use gpui::TestAppContext;
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| {
        app.voice_settings
            .select_desktop_mode(DesktopVoiceMode::GptLive);
        let (generation, media) = app.gpt_live.begin();
        let (commands, controls) = async_channel::bounded(1);
        app.gpt_live.commands = Some(commands);
        app.gpt_live.apply(generation, CallEvent::Connected);
        app.select_desktop_voice_mode(DesktopVoiceMode::GptLive, cx);
        assert!(!media.is_cancelled(), "重复选择当前模式不取消通话");
        let view = app.gpt_live.view();
        assert!(view.active && view.can_mute, "已连接快照可静音");
        assert!(!view.muted, "初始未静音");
        app.toggle_gpt_live_mute(cx);
        assert!(
            matches!(controls.try_recv(), Ok(CallCommand::SetMuted(true))),
            "按钮发送真实静音命令"
        );
        assert!(
            !app.gpt_live.view().can_mute,
            "等待本地媒体循环确认时禁用重复操作"
        );
        app.gpt_live.apply(generation, CallEvent::Muted(true));
        assert!(
            app.gpt_live.view().muted && app.gpt_live.view().can_mute,
            "本地gate更新后显示静音且可恢复"
        );
        app.select_desktop_voice_mode(DesktopVoiceMode::Legacy, cx);
        assert!(media.is_cancelled(), "切换模式取消媒体token");
        assert!(!app.gpt_live.view().active, "切换后快照不可保持通话");
        assert!(!app.gpt_live.view().can_mute, "无通话不能静音");
        assert!(
            app.gpt_live.view().usage.is_none() && app.gpt_live.view().usage_pending,
            "未收到用量时保持未确认，不伪造0秒"
        );
    });
}

/// AC11/AC13：无关设置保存失败后仍需追上另一窗口已撤销的Live Key。
/// 数据构造：隔离SQLite旧Live引用、待写BaseUrl，以及真实SQL触发器使该无关写入失败。
/// 执行过程：远端删Key→本实例因pending写跳过同步→执行失败写入→生产完成回调再次同步。
/// 预期结果：旧媒体token取消且旧引用移除，不能因无关失败永远保留已撤销的凭据。
#[test]
fn gpt_live_remote_revocation_is_resynced_after_unrelated_write_failure() {
    use crate::{
        app::MultisoulApp,
        settings::voice::{VoiceSettingsField, VoiceSettingsState},
    };
    use gpui::TestAppContext;
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("settings.sqlite"));
    store
        .set_string("voice.desktop.mode", "gpt_live")
        .expect("GPT模式");
    store
        .set_string("voice.gpt_live.credential_ref", "gpt-live-desktop-old")
        .expect("旧引用");
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let media = app.update(cx, |app, _| {
        app.voice_settings = VoiceSettingsState::load_from_store(store.clone());
        let (_, media) = app.gpt_live.begin();
        app.voice_settings.begin_edit_with_value(
            VoiceSettingsField::BaseUrl,
            "https://api.example.com/v1".into(),
        );
        assert!(
            app.voice_settings
                .auto_save_edit_value(
                    VoiceSettingsField::BaseUrl,
                    "https://example.invalid/v1".into()
                )
                .is_some(),
            "无关设置进入待保存"
        );
        media
    });
    store
        .delete_string("voice.gpt_live.credential_ref")
        .expect("另一窗口撤销凭据");
    app.update(cx, |app, cx| app.resync_voice_settings(false, cx));
    cx.run_until_parked();
    assert!(
        !media.is_cancelled(),
        "本轮同步确实因pending保存而跳过，覆盖竞态前置条件"
    );
    rusqlite::Connection::open(store.path()).expect("SQLite").execute_batch("CREATE TRIGGER reject_settings BEFORE INSERT ON settings BEGIN SELECT RAISE(ABORT, 'fixture write failure'); END;").expect("实际失败夹具");
    app.update(cx, |app, cx| app.persist_voice_settings_in_background(cx));
    cx.run_until_parked();
    assert!(
        media.is_cancelled(),
        "无关写失败后也必须追上Key撤销并取消旧媒体"
    );
    app.read_with(cx, |app, _| {
        assert!(
            app.voice_settings
                .values()
                .gpt_live_profile()
                .credential_ref()
                .is_none(),
            "已撤销引用不能停留在本地快照"
        );
        assert!(!app.gpt_live.is_active(), "旧通话已经退出活跃状态");
    });
}

/// AC11：无项目的桌面Host关窗保留后台时，也必须停止该Host自己的实时通话。
/// 数据构造：隔离Host实体和Live媒体token；无子项目，无真实设备/网络。
/// 执行过程：begin→生产detach_desktop_host_window→保留实体并核对媒体状态。
/// 预期结果：关窗立即取消Host token、状态inactive；不依赖实体Drop完成取消。
#[test]
fn gpt_live_desktop_host_detach_stops_call_without_dropping_host() {
    use crate::app::MultisoulApp;
    use gpui::TestAppContext;
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| {
        let (_, media) = app.gpt_live.begin();
        assert!(app.gpt_live.is_active(), "Host已开始通话");
        app.detach_desktop_host_window(cx);
        assert!(media.is_cancelled(), "Host实体仍存活时关窗也须取消媒体");
        assert!(!app.gpt_live.is_active(), "关窗后的Host不能继续采集或计费");
        assert!(!app.window_active, "窗口已分离");
    });
}

/// AC11：中间用量是累计快照，不是增量，也不能在没有上游报告时伪造0秒。
/// 数据构造：新通话、1.25/3.5秒累计Usage及重复快照，随后开启新generation。
/// 执行过程：连接前检查未报告→应用两快照与重复→重开→重放旧Usage。
/// 预期结果：报告前无数字；最终中间值3.5而非累加；旧generation不污染新通话。
#[test]
fn gpt_live_view_uses_reported_cumulative_usage_without_inventing_zero() {
    let mut state = GptLiveVoiceState::default();
    let (first, _) = state.begin();
    assert!(
        state.view().usage.is_none(),
        "未收到任何服务端报告时不能显示已报告0秒"
    );
    for seconds in [1.25, 3.5, 3.5] {
        assert!(
            state.apply(
                first,
                CallEvent::Usage(LiveUsage {
                    seconds,
                    finalized: false
                })
            ),
            "本轮累计用量可更新"
        );
    }
    assert_eq!(
        state.view().usage.expect("已收到快照").seconds,
        3.5,
        "累计快照替换，不累加"
    );
    state.begin();
    assert!(
        !state.apply(
            first,
            CallEvent::Usage(LiveUsage {
                seconds: 99.0,
                finalized: false
            })
        ),
        "旧代用量拒绝"
    );
    assert!(state.view().usage.is_none(), "新通话仍未报告用量");
}

/// AC11：worker异常结束的未知用量不能抹掉已收到的累计快照。
/// 数据构造：已报告3.5秒但未closed的通话，随后收到默认0秒的错误终态。
/// 执行过程：begin→Usage3.5→模拟oneshot丢失的CallEnd(default,error)→读取快照。
/// 预期结果：保留3.5秒并标未确认，错误可见；不能倒退到伪造0秒。
#[test]
fn gpt_live_unknown_worker_end_preserves_last_reported_usage() {
    let mut state = GptLiveVoiceState::default();
    let (generation, _) = state.begin();
    state.apply(
        generation,
        CallEvent::Usage(LiveUsage {
            seconds: 3.5,
            finalized: false,
        }),
    );
    state.finish(
        generation,
        CallEnd {
            usage: LiveUsage::default(),
            error: Some("worker ended unexpectedly".into()),
        },
    );
    let usage = state.view().usage.expect("已有可信累计快照");
    assert_eq!(usage.seconds, 3.5, "未知终态不能回退已报告用量");
    assert!(!usage.finalized, "没有closed仍未确认最终用量");
    assert!(
        state.caption().contains("worker ended unexpectedly"),
        "异常原因仍显示"
    );
    assert!(!state.is_active(), "异常通话不保持活跃");
}

/// AC07/AC11：项目A的核对文本不能在项目B或个人会话显示。
/// 构造：A的真实核对结果存入状态；分别构造A/B/无项目的设置View。
/// 执行：调用生产view_for_project入口。
/// 预期：只有A可见；B和个人会话无旧项目正文。
#[test]
fn gpt_live_reconciliation_view_is_scoped_to_current_project() {
    let state = GptLiveVoiceState {
        reconciliation: Some(("/a".into(), "A项目原命令".into())),
        ..Default::default()
    };
    assert_eq!(
        state
            .view_for_project(Some(std::path::Path::new("/a")))
            .reconciliation
            .as_deref(),
        Some("A项目原命令"),
        "同项目可核对"
    );
    assert_eq!(
        state
            .view_for_project(Some(std::path::Path::new("/b")))
            .reconciliation,
        None,
        "其他项目不可见"
    );
    assert_eq!(
        state.view_for_project(None).reconciliation,
        None,
        "个人会话不可见项目原文"
    );
}

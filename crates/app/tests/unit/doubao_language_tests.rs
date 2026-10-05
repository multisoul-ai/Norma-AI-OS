use super::{connect_failure_message, plan::asr};
use ui::i18n::Locale;

/// 已发生的豆包连接错误在切换语言后重新展示，鉴权细节保持明确。
///
/// 数据构造：HTTP 401 表示密钥鉴权失败，不包含响应体，不执行网络请求。
/// 执行过程：英文生成并缓存错误 → 中文读取同一错误 → 恢复英文。
/// 预期结果：英文提示 ASR 2.0 authentication；中文提示方舟 API Key；英文不残留中文。
#[test]
fn doubao_cached_handshake_failure_tracks_locale() {
    let response = tokio_tungstenite::tungstenite::http::Response::builder()
        .status(401)
        .body(None)
        .expect("应能构造未鉴权响应");
    Locale::En.apply();
    let message = connect_failure_message(&tokio_tungstenite::tungstenite::Error::Http(Box::new(
        response,
    )));
    assert_eq!(
        message.to_string(),
        "Ark API Key failed ASR 2.0 authentication; check the key and resource permissions",
        "英文鉴权错误应有明确原因与恢复建议"
    );
    Locale::ZhCn.apply();
    assert_eq!(
        message.to_string(),
        "方舟 API Key 未通过 ASR 2.0 鉴权；请检查 Key 与资源权限",
        "已缓存鉴权错误必须立即切换中文"
    );
    Locale::En.apply();
    assert!(
        !message.to_string().contains("方舟"),
        "英文错误不得残留中文产品文案"
    );
}

/// 已缓存的 ASR 协议错误不会固定在生成时的语言。
///
/// 数据构造：空帧 0 字节，小于协议最小帧头 12 字节，因此必定在帧头校验失败。
/// 执行过程：中文解码空帧 → 保留错误对象 → 切英文 → 恢复英文默认值。
/// 预期结果：中文为无效帧头；英文为 invalid frame header；不得保留中文诊断。
#[test]
fn doubao_cached_protocol_failure_tracks_locale() {
    Locale::ZhCn.apply();
    let error = asr::decode(&[]).expect_err("空帧不能解码成功");
    assert_eq!(
        error.to_string(),
        "ASR 返回了无效帧头",
        "中文协议错误应明确指出帧头无效"
    );
    Locale::En.apply();
    assert_eq!(
        error.to_string(),
        "ASR returned an invalid frame header",
        "缓存协议错误切换后必须使用英文"
    );
    assert_ne!(
        error.to_string(),
        "ASR 返回了无效帧头",
        "英文不得保留生成错误时的中文文案"
    );
}

/// 豆包底栏复用实时状态，两种语言切换不会重启语音会话。
///
/// 数据构造：TestAppContext 的单个窗口，echo PTY；豆包为 Thinking 后转 Disconnected。
/// 执行过程：选择豆包模式 → 中文读取思考状态 → 英文读取同一状态 → 断开后中文读取。
/// 预期结果：底栏和完整标签均随语言更新；状态仍为 Thinking；断开不能残留英文。
#[test]
fn doubao_dock_status_tracks_locale_without_resetting_state() {
    use crate::app::MultisoulApp;
    use crate::settings::voice::DesktopVoiceMode;
    use gpui::{AppContext, TestAppContext};
    let mut cx = TestAppContext::single();
    let app = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(&mut cx, |app, _| {
        app.voice_settings
            .select_desktop_mode(DesktopVoiceMode::Doubao);
        app.doubao.status = super::DoubaoStatus::Thinking;
        Locale::ZhCn.apply();
        assert_eq!(
            app.effective_voice_dock_label(),
            "思考中…",
            "中文底栏应翻译豆包思考状态"
        );
        assert_eq!(
            app.effective_voice_label(),
            "思考中…",
            "完整标签应与底栏一致"
        );
        Locale::En.apply();
        assert_eq!(
            app.effective_voice_dock_label(),
            "Thinking…",
            "相同状态应立即显示英文"
        );
        assert_eq!(
            app.doubao.status,
            super::DoubaoStatus::Thinking,
            "切换语言不能重置语音状态"
        );
        app.doubao.status = super::DoubaoStatus::Disconnected("HTTP 401".into());
        Locale::ZhCn.apply();
        assert_eq!(
            app.effective_voice_dock_label(),
            "语音已断开",
            "中文应说明语音断开"
        );
        assert_ne!(
            app.effective_voice_label(),
            "Voice disconnected",
            "断开标签不得残留英文"
        );
        Locale::En.apply();
    });
}

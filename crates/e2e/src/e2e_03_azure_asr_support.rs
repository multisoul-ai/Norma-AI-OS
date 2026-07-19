use gpui::{
    point, px, ClipboardItem, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ScrollDelta,
    ScrollWheelEvent,
};
use std::io::{Read, Write};
use std::net::TcpListener;

pub(super) fn voice_key_down(cx: &mut gpui::VisualTestContext) {
    cx.simulate_event(KeyDownEvent {
        keystroke: voice_keystroke(),
        is_held: false,
    });
    cx.run_until_parked();
}

pub(super) fn voice_key_up(cx: &mut gpui::VisualTestContext) {
    cx.simulate_event(KeyUpEvent {
        keystroke: voice_keystroke(),
    });
    cx.run_until_parked();
}

fn voice_keystroke() -> Keystroke {
    Keystroke {
        modifiers: Modifiers {
            platform: true,
            ..Modifiers::default()
        },
        key: "e".to_string(),
        key_char: None,
    }
}

pub(super) fn wait_for_active_speech_session(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
) {
    for _ in 0..600 {
        cx.run_until_parked();
        if app.read_with(cx, |app, _cx| app.voice_has_active_speech_session_for_e2e()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!(
        "Azure Single Voice Command should install its speech session; snapshot: {:?}",
        app.read_with(cx, |app, _cx| app.voice_e2e_snapshot())
    );
}

pub(super) fn wait_for_pending_transcript(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    expected: &str,
) {
    for _ in 0..600 {
        cx.cx
            .background_executor
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
        if app.read_with(cx, |app, _cx| {
            app.pending_voice_transcript_for_e2e().as_deref() == Some(expected)
        }) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!(
        "expected Azure transcript {expected:?}; actual: {:?}; snapshot: {:?}",
        app.read_with(cx, |app, _cx| app.pending_voice_transcript_for_e2e()),
        app.read_with(cx, |app, _cx| app.voice_e2e_snapshot())
    );
}

pub(super) fn wait_for_voice_phase(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    expected: &str,
) {
    for _ in 0..600 {
        cx.cx
            .background_executor
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
        if app.read_with(cx, |app, _cx| app.voice_e2e_snapshot().phase == expected) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!(
        "expected Voice phase {expected:?}; snapshot: {:?}",
        app.read_with(cx, |app, _cx| app.voice_e2e_snapshot())
    );
}

pub(super) fn input_text(cx: &mut gpui::VisualTestContext, selector: &'static str, value: &str) {
    click(cx, selector);
    cx.simulate_input(value);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

pub(super) fn input_secret(cx: &mut gpui::VisualTestContext, selector: &'static str, value: &str) {
    click(cx, selector);
    cx.update(|_window, cx| cx.write_to_clipboard(ClipboardItem::new_string(value.to_string())));
    cx.simulate_keystrokes("cmd-v enter");
    cx.run_until_parked();
}

pub(super) fn open_voice_settings(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
) {
    let settings_open = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx).settings_open);
    if !settings_open {
        click(cx, "toolbar-settings");
    }
    click(cx, "settings-section-voice");
}

pub(super) fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    scroll_settings_target_into_view(cx, selector);
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("expected visible selector {selector}"));
    if selector.starts_with("settings-asr-") || selector.starts_with("settings-voice-") {
        if let Some(content) = cx.debug_bounds("settings-content-scroll") {
            assert!(
                bounds.center().y >= content.origin.y
                    && bounds.center().y <= content.origin.y + content.size.height,
                "selector {selector} must be inside the Settings viewport before click; target={bounds:?}, viewport={content:?}"
            );
        }
    }
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn scroll_settings_target_into_view(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    // Voice Settings keeps its scroll position across close/reopen. Later assertions can
    // therefore start more than 2,000 px away from the provider controls, while the GPUI
    // test scroll path advances in bounded steps.
    for _ in 0..20 {
        let Some(content) = cx.debug_bounds("settings-content-scroll") else {
            return;
        };
        let Some(target) = cx.debug_bounds(selector) else {
            return;
        };
        let delta = f32::from(content.center().y) - f32::from(target.center().y);
        if delta.abs() <= 80.0 {
            return;
        }
        cx.simulate_mouse_move(content.center(), None, Modifiers::none());
        cx.simulate_event(ScrollWheelEvent {
            position: content.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(delta))),
            ..Default::default()
        });
        cx.run_until_parked();
    }
}

pub(super) fn spawn_fake_azure_server() -> (String, std::thread::JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake Azure should bind a local port");
    let endpoint = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("fake Azure should expose its local address")
    );
    let server = std::thread::spawn(move || {
        let responses = [
            (
                "200 OK",
                r#"{"RecognitionStatus":"InitialSilenceTimeout","DisplayText":""}"#,
                0,
            ),
            (
                "200 OK",
                r#"{"RecognitionStatus":"Success","DisplayText":"Azure e2e transcript"}"#,
                0,
            ),
            (
                "200 OK",
                r#"{"RecognitionStatus":"Success","DisplayText":"Azure single transcript"}"#,
                0,
            ),
            (
                "200 OK",
                r#"{"RecognitionStatus":"Success","DisplayText":"Azure continuous transcript"}"#,
                0,
            ),
            (
                "200 OK",
                r#"{"RecognitionStatus":"Success","DisplayText":"Azure limit transcript"}"#,
                300,
            ),
            (
                "401 Unauthorized",
                r#"{"error":{"code":"Unauthorized"}}"#,
                0,
            ),
            (
                "200 OK",
                r#"{"RecognitionStatus":"Success","DisplayText":"Azure stale transcript"}"#,
                300,
            ),
            (
                "200 OK",
                r#"{"RecognitionStatus":"Success","DisplayText":"Azure fresh transcript"}"#,
                0,
            ),
        ];
        responses
            .into_iter()
            .map(|(status, body, delay_ms)| {
                let (mut stream, _) = listener.accept().expect("fake Azure should accept request");
                let request = read_http_request(&mut stream);
                if delay_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                }
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .expect("fake Azure should write its full response");
                request
            })
            .collect()
    });
    (endpoint, server)
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .expect("fake Azure should use a bounded read timeout");
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    while !http_request_complete(&request) {
        let count = stream
            .read(&mut buffer)
            .expect("fake Azure should read request");
        if count == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..count]);
    }
    request
}

fn http_request_complete(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    request.len() >= header_end + 4 + content_length
}

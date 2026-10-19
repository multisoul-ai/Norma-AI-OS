//! Line-delimited JSON requests for the debug-only desktop E2E control channel.
use gpui::{
    point, px, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PlatformInput, ScrollDelta, ScrollWheelEvent, TouchPhase,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MIN_TOKEN_LEN: usize = 32;

pub(super) struct ControlConfig {
    pub(super) socket: PathBuf,
    pub(super) token: String,
}

pub(super) fn control_config(
    socket: Option<String>,
    token: Option<String>,
) -> Option<ControlConfig> {
    let socket = socket.filter(|socket| !socket.is_empty())?;
    let token = token.filter(|token| token.len() >= MIN_TOKEN_LEN)?;
    Some(ControlConfig {
        socket: PathBuf::from(socket),
        token,
    })
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum MousePhase {
    Move,
    Down,
    Up,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(super) enum Command {
    Window,
    Activate,
    Mouse {
        phase: MousePhase,
        x: f32,
        y: f32,
        #[serde(default = "single_click")]
        click_count: usize,
        #[serde(default)]
        pressed: bool,
        #[serde(default)]
        modifiers: Vec<String>,
    },
    Scroll {
        x: f32,
        y: f32,
        dy: f32,
    },
    Key {
        keystroke: String,
    },
    Text {
        text: String,
    },
    Resize {
        width: f32,
        height: f32,
    },
    Capture {
        path: PathBuf,
    },
}

fn single_click() -> usize {
    1
}

#[derive(Deserialize)]
struct Request {
    token: String,
    #[serde(flatten)]
    command: Command,
}

pub(super) fn parse_request(line: &str, expected_token: &str) -> Result<Command, String> {
    let request: Request =
        serde_json::from_str(line).map_err(|error| format!("invalid request: {error}"))?;
    if !tokens_match(request.token.as_bytes(), expected_token.as_bytes()) {
        return Err("unauthorized".into());
    }
    Ok(request.command)
}

fn tokens_match(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct WindowInfo {
    pub(super) number: i64,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) scale: f32,
}

#[derive(Debug, Serialize)]
pub(super) struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    window: Option<WindowInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Response {
    pub(super) fn ok() -> Self {
        Self {
            ok: true,
            window: None,
            error: None,
        }
    }

    pub(super) fn window(window: WindowInfo) -> Self {
        Self {
            ok: true,
            window: Some(window),
            error: None,
        }
    }

    pub(super) fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            window: None,
            error: Some(message.into()),
        }
    }

    pub(super) fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"ok":false}"#.into())
    }
}

fn modifiers(names: &[String]) -> Modifiers {
    let mut modifiers = Modifiers::none();
    for name in names {
        match name.as_str() {
            "cmd" => modifiers.platform = true,
            "shift" => modifiers.shift = true,
            "alt" => modifiers.alt = true,
            "ctrl" => modifiers.control = true,
            _ => {}
        }
    }
    modifiers
}

pub(super) fn mouse_input(
    phase: MousePhase,
    x: f32,
    y: f32,
    click_count: usize,
    pressed: bool,
    modifier_names: &[String],
) -> PlatformInput {
    let position = point(px(x), px(y));
    let modifiers = modifiers(modifier_names);
    match phase {
        MousePhase::Move => PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: pressed.then_some(MouseButton::Left),
            modifiers,
        }),
        MousePhase::Down => PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count,
            first_mouse: false,
        }),
        MousePhase::Up => PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count,
        }),
    }
}

pub(super) fn scroll_input(x: f32, y: f32, dy: f32) -> PlatformInput {
    PlatformInput::ScrollWheel(ScrollWheelEvent {
        position: point(px(x), px(y)),
        delta: ScrollDelta::Pixels(point(px(0.0), px(dy))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    })
}

pub(super) fn text_keystrokes(text: &str) -> Vec<Keystroke> {
    text.chars()
        .map(|character| {
            let typed = character.to_string();
            let lower = character.to_lowercase().to_string();
            Keystroke {
                modifiers: Modifiers {
                    shift: character.is_uppercase(),
                    ..Modifiers::none()
                },
                key: lower,
                key_char: Some(typed),
            }
        })
        .collect()
}

//! Debug-build-only control channel for `scripts/desktop-e2e.sh`.
//!
//! The driver injects input and reads the App's own window image through this
//! socket instead of system-wide CGEvent/screen capture, so runs need neither
//! Accessibility nor Screen Recording permission. It starts only when both
//! `NORMA_E2E_CONTROL_SOCKET` and a per-run `NORMA_E2E_CONTROL_TOKEN` are set,
//! and is not compiled into release builds.
mod capture;
mod ns_input;
mod placement;

pub(crate) use placement::target_display;
use placement::{park_window, stay_in_background};
mod protocol;

#[cfg(test)]
#[path = "../../tests/unit/e2e_control_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/e2e_text_input_tests.rs"]
mod text_input_tests;

use gpui::{
    px, size, AnyWindowHandle, App, AsyncApp, KeyUpEvent, Keystroke, PlatformInput, Window,
};
use protocol::{
    control_config, modifiers_input, mouse_input, parse_request, scroll_input, text_keystrokes,
    Command, Response, WindowInfo,
};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

const SOCKET_ENV: &str = "NORMA_E2E_CONTROL_SOCKET";
const TOKEN_ENV: &str = "NORMA_E2E_CONTROL_TOKEN";

struct Job {
    command: Command,
    reply: async_channel::Sender<Outcome>,
}

enum Outcome {
    Done(Response),
    Capture { window: WindowInfo, path: PathBuf },
}

pub(crate) fn install(cx: &mut App) {
    let Some(config) = control_config(
        std::env::var(SOCKET_ENV).ok(),
        std::env::var(TOKEN_ENV).ok(),
    ) else {
        return;
    };
    let listener = match bind(&config.socket) {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%error, socket = %config.socket.display(), "desktop E2E control socket unavailable");
            return;
        }
    };
    let (jobs, receiver) = async_channel::unbounded::<Job>();
    let token = config.token;
    let spawned = std::thread::Builder::new()
        .name("norma-e2e-control".into())
        .spawn(move || serve(listener, token, jobs));
    if let Err(error) = spawned {
        tracing::error!(%error, "desktop E2E control thread unavailable");
        return;
    }
    if !foreground_requested() {
        stay_in_background();
    }
    cx.spawn(async move |cx| {
        while let Ok(job) = receiver.recv().await {
            let needs_frame = matches!(
                job.command,
                Command::Activate
                    | Command::Mouse { .. }
                    | Command::NsMouse { .. }
                    | Command::Capture { .. }
            );
            let mut outcome = execute_command(job.command, cx);
            if needs_frame {
                if let Err(error) = settle_frames(cx).await {
                    outcome = Outcome::Done(Response::error(error));
                }
            }
            let _ = job.reply.send(outcome).await;
        }
    })
    .detach();
    tracing::warn!(socket = %config.socket.display(), "desktop E2E control channel enabled");
}

fn execute_command(command: Command, cx: &mut AsyncApp) -> Outcome {
    match command {
        Command::Key { keystroke } => match Keystroke::parse(&keystroke) {
            Ok(key) => execute_keys(vec![key], cx),
            Err(error) => Outcome::Done(Response::error(error.to_string())),
        },
        Command::Text { text } => execute_keys(text_keystrokes(&text), cx),
        Command::NsMouse { .. } | Command::NsText { .. } | Command::NsKey { .. } => {
            execute_native(command, cx)
        }
        command => cx
            .update(|cx| execute(command, cx))
            .unwrap_or_else(|_| Outcome::Done(Response::error("app unavailable"))),
    }
}

/// Each native event ends an App update before the next event can redraw the window.
/// Keep that boundary here too: a KeyUp redraw must see InputEvent::Change effects
/// from KeyDown, otherwise controlled inputs can restore their stale model value.
fn execute_keys(keys: Vec<Keystroke>, cx: &mut AsyncApp) -> Outcome {
    let handle = match cx.update(target_window) {
        Ok(Some(handle)) => handle,
        Ok(None) => return Outcome::Done(Response::error("no window")),
        Err(_) => return Outcome::Done(Response::error("app unavailable")),
    };
    for (index, key) in keys.into_iter().enumerate() {
        for phase in ["down", "up"] {
            let outcome = cx.update(|cx| {
                handle.update(cx, |_, window, cx| {
                    if phase == "down" {
                        window.dispatch_keystroke(key.clone(), cx);
                    } else {
                        window.dispatch_event(
                            PlatformInput::KeyUp(KeyUpEvent {
                                keystroke: key.clone(),
                            }),
                            cx,
                        );
                    }
                })
            });
            let error = match outcome {
                Ok(Ok(())) => continue,
                Ok(Err(_)) => "window closed",
                Err(_) => "app unavailable",
            };
            return Outcome::Done(Response::error(format!("key[{index}] {phase}: {error}")));
        }
    }
    Outcome::Done(Response::ok())
}

fn bind(path: &Path) -> std::io::Result<UnixListener> {
    if path.symlink_metadata().is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "control socket path already exists",
        ));
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

fn serve(listener: UnixListener, token: String, jobs: async_channel::Sender<Job>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if !peer_is_same_user(&stream) {
            continue;
        }
        let token = token.clone();
        let jobs = jobs.clone();
        let _ = std::thread::Builder::new()
            .name("norma-e2e-control-conn".into())
            .spawn(move || handle_connection(stream, &token, &jobs));
    }
}

fn peer_is_same_user(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut uid = 0;
    let mut gid = 0;
    let status = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    status == 0 && uid == unsafe { libc::geteuid() }
}

fn handle_connection(stream: UnixStream, token: &str, jobs: &async_channel::Sender<Job>) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let response = match parse_request(&line, token) {
            Ok(command) => run(command, jobs),
            Err(error) => Response::error(error),
        };
        if writeln!(writer, "{}", response.to_line()).is_err() {
            return;
        }
    }
}

fn run(command: Command, jobs: &async_channel::Sender<Job>) -> Response {
    let (reply, outcome) = async_channel::bounded(1);
    if jobs.send_blocking(Job { command, reply }).is_err() {
        return Response::error("app unavailable");
    }
    match outcome.recv_blocking() {
        Ok(Outcome::Done(response)) => response,
        Ok(Outcome::Capture { window, path }) => capture_to(window, &path),
        Err(_) => Response::error("app unavailable"),
    }
}

fn capture_to(window: WindowInfo, path: &Path) -> Response {
    let Ok(number) = u32::try_from(window.number) else {
        return Response::error("invalid window number");
    };
    match capture::capture_window_png(number, path) {
        Ok(()) => Response::window(window),
        Err(error) => Response::error(error),
    }
}

fn target_window(cx: &mut App) -> Option<AnyWindowHandle> {
    let mut best: Option<(AnyWindowHandle, f32)> = None;
    for handle in cx.windows() {
        let Ok(area) = handle.update(cx, |_, window, _| {
            let size = window.bounds().size;
            f32::from(size.width) * f32::from(size.height)
        }) else {
            continue;
        };
        if best.is_none_or(|(_, largest)| area > largest) {
            best = Some((handle, area));
        }
    }
    best.map(|(handle, _)| handle)
}

/// AppKit input is delivered after `update` returns so GPUI can re-enter the app.
fn execute_native(command: Command, cx: &mut AsyncApp) -> Outcome {
    let native = cx
        .update(|cx| {
            let handle = target_window(cx).ok_or("no window")?;
            handle
                .update(cx, |_, window, _| ns_input::native_window(window))
                .map_err(|_| "window closed")?
        })
        .unwrap_or(Err("app unavailable"));
    let native = match native {
        Ok(native) => native,
        Err(error) => return Outcome::Done(Response::error(error)),
    };
    Outcome::Done(match command {
        Command::NsMouse {
            phase,
            x,
            y,
            click_count,
            pressed,
        } => ns_input::mouse(&native, phase, x, y, click_count, pressed),
        Command::NsText { text } => ns_input::text(&native, &text),
        Command::NsKey {
            key_code,
            chars,
            modifiers,
        } => ns_input::key(&native, key_code, &chars, &modifiers),
        _ => Response::error("not a native command"),
    })
}

fn execute(command: Command, cx: &mut App) -> Outcome {
    let Some(handle) = target_window(cx) else {
        return Outcome::Done(Response::error("no window"));
    };
    handle
        .update(cx, |_, window, cx| execute_in(command, window, cx))
        .unwrap_or_else(|_| Outcome::Done(Response::error("window closed")))
}

fn execute_in(command: Command, window: &mut Window, cx: &mut App) -> Outcome {
    let done = |response| Outcome::Done(response);
    match command {
        Command::Window => done(window_info(window).map_or_else(Response::error, Response::window)),
        Command::Activate => {
            if foreground_requested() {
                cx.activate(true);
                window.activate_window();
            } else {
                window.set_active_for_e2e(cx);
                park_window(window);
            }
            done(Response::ok())
        }
        Command::Deactivate => {
            window.set_inactive_for_e2e(cx);
            done(Response::ok())
        }
        Command::Mouse {
            phase,
            x,
            y,
            click_count,
            pressed,
            modifiers,
        } => {
            window.dispatch_event(
                mouse_input(phase, x, y, click_count, pressed, &modifiers),
                cx,
            );
            done(Response::ok())
        }
        Command::Scroll { x, y, dy } => {
            window.dispatch_event(scroll_input(x, y, dy), cx);
            done(Response::ok())
        }
        Command::Key { .. } | Command::Text { .. } => done(Response::error(
            "keyboard input requires an App update boundary between events",
        )),
        Command::Modifiers { modifiers } => {
            window.dispatch_event(modifiers_input(&modifiers), cx);
            done(Response::ok())
        }
        Command::Resize { width, height } => {
            window.resize(size(px(width), px(height)));
            done(Response::ok())
        }
        Command::NsMouse { .. } | Command::NsText { .. } | Command::NsKey { .. } => done(
            Response::error("native input runs outside the window lease"),
        ),
        Command::DoubaoTranscript { text } => {
            let injected = crate::workspace::desktop::sessions::windowed(cx)
                .ok_or_else(|| "no windowed project host".to_string())
                .and_then(|app| {
                    app.update(cx, |app, cx| app.inject_doubao_transcript(text, window, cx))
                });
            done(match injected {
                Ok(()) => Response::ok(),
                Err(error) => Response::error(error),
            })
        }
        Command::VoiceTranscript { text } => {
            let host = crate::workspace::desktop::sessions::windowed(cx);
            if let Some(host) = host {
                host.update(cx, |app, cx| {
                    let expanded = app
                        .desktop_workspace
                        .active_id
                        .as_ref()
                        .zip(app.desktop_workspace.expanded_area_id.as_ref())
                        .and_then(|(workspace, area)| {
                            app.desktop_workspace
                                .sessions
                                .get(&(workspace.clone(), area.clone()))
                                .cloned()
                        });
                    if let Some(expanded) = expanded {
                        expanded
                            .update(cx, |app, cx| app.handle_final_voice_text(text, window, cx));
                    } else {
                        app.handle_final_voice_text(text, window, cx);
                    }
                });
                done(Response::ok())
            } else {
                done(Response::error("no windowed project host"))
            }
        }
        Command::RunEffectsFixture => {
            let installed = crate::workspace::desktop::sessions::windowed(cx)
                .ok_or_else(|| "no windowed host".to_string())
                .map(|app| {
                    app.update(cx, |app, cx| {
                        crate::app::run_effects_fixture::install(app);
                        cx.notify();
                    })
                });
            done(match installed {
                Ok(()) => Response::ok(),
                Err(error) => Response::error(error),
            })
        }
        Command::Capture { path } => match window_info(window) {
            Ok(window) => Outcome::Capture { window, path },
            Err(error) => done(Response::error(error)),
        },
    }
}

/// ACK input only after a GPUI draw/present submission cycle. GPUI installs MouseUp listeners
/// while painting the pressed state, so Down and Up must not share the same old frame.
/// Capture uses the same barrier before reading the window; visual checks must still
/// verify pixels because Metal/WindowServer presentation is asynchronous.
async fn settle_frames(cx: &mut AsyncApp) -> Result<(), &'static str> {
    let (painted, wait) = async_channel::bounded::<()>(1);
    let queued = cx
        .update(|cx| {
            let Some(handle) = target_window(cx) else {
                return false;
            };
            handle
                .update(cx, |_, window, _| {
                    window.refresh();
                    window.on_next_frame(move |window, _| {
                        // Callbacks run before draw. Refresh here, then acknowledge on
                        // the next callback: the first frame has now drawn/presented.
                        window.refresh();
                        window.on_next_frame(move |_, _| {
                            let _ = painted.try_send(());
                        });
                    });
                })
                .is_ok()
        })
        .unwrap_or(false);
    if !queued {
        return Err("E2E frame barrier: window unavailable");
    }
    let timeout = cx.background_executor().timer(FRAME_SETTLE_TIMEOUT);
    futures::pin_mut!(timeout);
    let wait = wait.recv();
    futures::pin_mut!(wait);
    match futures::future::select(wait, timeout).await {
        futures::future::Either::Left((Ok(()), _)) => Ok(()),
        futures::future::Either::Left((Err(_), _)) => {
            Err("E2E frame barrier: window closed before paint")
        }
        futures::future::Either::Right(_) => Err("E2E frame barrier: paint timed out after 2s"),
    }
}

const FRAME_SETTLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// `NORMA_E2E_FOREGROUND=1` restores the old behaviour of taking the OS foreground.
pub(super) fn foreground_requested() -> bool {
    placement::flag_enabled(std::env::var("NORMA_E2E_FOREGROUND").ok().as_deref())
}

fn window_info(window: &Window) -> Result<WindowInfo, &'static str> {
    let size = window.bounds().size;
    Ok(WindowInfo {
        number: native_window_number(window)?,
        width: f32::from(size.width),
        height: f32::from(size.height),
        scale: window.scale_factor(),
    })
}

#[cfg(not(test))]
fn native_window_number(window: &Window) -> Result<i64, &'static str> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).map_err(|_| "no native window")?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("not an AppKit window");
    };
    // SAFETY: GPUI owns this live view and we are on the AppKit main thread.
    let view = unsafe { &*handle.ns_view.as_ptr().cast::<objc2_app_kit::NSView>() };
    view.window()
        .map(|window| window.windowNumber() as i64)
        .ok_or("view has no window")
}

#[cfg(test)]
fn native_window_number(_: &Window) -> Result<i64, &'static str> {
    Err("no native window")
}

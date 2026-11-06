//! Native window selection only: no model state or product actions on the test channel.
use super::{
    native_window_number,
    protocol::{Command, Response},
    window_info,
};
use gpui::{AnyWindowHandle, App, Global};

#[derive(Default)]
struct SelectedWindow(Option<i64>);
impl Global for SelectedWindow {}

pub(super) fn target_window(cx: &mut App) -> Option<AnyWindowHandle> {
    let selected = cx
        .try_global::<SelectedWindow>()
        .and_then(|selection| selection.0);
    let mut best = None;
    for handle in cx.windows() {
        let Ok((number, area)) = handle.update(cx, |_, window, _| {
            let size = window.bounds().size;
            (
                native_window_number(window).ok(),
                f32::from(size.width) * f32::from(size.height),
            )
        }) else {
            continue;
        };
        if let Some(selected) = selected {
            if number == Some(selected) {
                return Some(handle);
            }
        } else if best.is_none_or(|(_, largest)| area > largest) {
            best = Some((handle, area));
        }
    }
    // An explicitly selected closed window must not redirect input to another window.
    best.map(|(handle, _)| handle)
}

pub(super) fn execute(command: Command, cx: &mut App) -> Response {
    match command {
        Command::Windows => {
            let windows = cx
                .windows()
                .into_iter()
                .filter_map(|handle| {
                    handle
                        .update(cx, |_, window, _| window_info(window))
                        .ok()?
                        .ok()
                })
                .collect();
            Response::windows(windows)
        }
        Command::SelectWindow { number } => {
            if let Some(number) = number {
                let exists = cx.windows().into_iter().any(|handle| {
                    handle
                        .update(cx, |_, window, _| {
                            native_window_number(window).ok() == Some(number)
                        })
                        .unwrap_or(false)
                });
                if !exists {
                    return Response::error("window not found");
                }
            }
            cx.set_global(SelectedWindow(number));
            Response::ok()
        }
        _ => Response::error("not a window selection command"),
    }
}

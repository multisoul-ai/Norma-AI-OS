use crate::app::MultisoulApp;
use gpui::{AnyWindowHandle, App, Entity, Global};
use std::path::{Path, PathBuf};

struct DesktopHost {
    database: PathBuf,
    window: Option<AnyWindowHandle>,
    app: Entity<MultisoulApp>,
}

#[derive(Default)]
struct DesktopHosts(Vec<DesktopHost>);
impl Global for DesktopHosts {}

pub(crate) fn hosts(database: &Path, cx: &App) -> Vec<Entity<MultisoulApp>> {
    cx.try_global::<DesktopHosts>()
        .map(|hosts| {
            hosts
                .0
                .iter()
                .filter(|host| host.database == database)
                .map(|host| host.app.clone())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn detached(database: &Path, cx: &App) -> Option<Entity<MultisoulApp>> {
    cx.try_global::<DesktopHosts>()?
        .0
        .iter()
        .find(|host| host.database == database && host.window.is_none())
        .map(|host| host.app.clone())
}

pub(crate) fn register(
    database: PathBuf,
    window: AnyWindowHandle,
    app: Entity<MultisoulApp>,
    cx: &mut App,
) {
    let hosts = &mut cx.default_global::<DesktopHosts>().0;
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.app.entity_id() == app.entity_id())
    {
        host.window = Some(window);
    } else {
        hosts.push(DesktopHost {
            database,
            window: Some(window),
            app,
        });
    }
}

pub(crate) fn windows_closed(retain_background: bool, cx: &mut App) {
    let windows = cx.windows();
    let hosts = std::mem::take(&mut cx.default_global::<DesktopHosts>().0);
    let mut kept = Vec::new();
    for mut host in hosts {
        if host.window.is_some_and(|window| !windows.contains(&window)) {
            if retain_background {
                host.app
                    .update(cx, |app, cx| app.detach_desktop_host_window(cx));
                host.window = None;
                kept.push(host);
            } else {
                host.app.update(cx, |app, cx| app.stop_desktop_sessions(cx));
            }
        } else {
            kept.push(host);
        }
    }
    cx.default_global::<DesktopHosts>().0 = kept;
}

pub(crate) fn shutdown(cx: &mut App) {
    for host in std::mem::take(&mut cx.default_global::<DesktopHosts>().0) {
        host.app.update(cx, |app, cx| app.stop_desktop_sessions(cx));
    }
}

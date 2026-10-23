//! 打开带门控的主窗口，并按 License 与账号状态决定首屏阶段。

use super::*;

pub fn open_gated_window(cx: &mut App) -> anyhow::Result<WindowHandle<Root>> {
    let license_phase = super::super::resolve::resolve_initial_boot_phase();
    let show_login = crate::account::install_account_controller(cx);
    let phase = super::super::boot::decide_phase_with_login(license_phase, show_login);
    open_gated_window_with_phase(phase, show_login, cx)
}

fn open_gated_window_with_phase(
    phase: BootPhase,
    login_after_activation: bool,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<Root>> {
    let title = title_for_initial_state(&InitialWindowState::Launchpad);
    let (width, height) = crate::workspace::windowing::initial_window_dimensions(
        crate::workspace::windowing::visual_preview_name(),
    );
    let display = crate::workspace::windowing::initial_display(cx);
    let bounds = gpui::Bounds::centered(display, gpui::size(px(width), px(height)), cx);
    let handle = cx.open_window(window_options(title, bounds, display), |window, cx| {
        cx.new(|cx| build_gated_root(phase, login_after_activation, window, cx))
    })?;
    Ok(handle)
}

fn build_gated_root(
    phase: BootPhase,
    login_after_activation: bool,
    window: &mut Window,
    cx: &mut Context<Root>,
) -> Root {
    let gate = cx.new(|cx| {
        let mut gate = GateRoot::new(phase, window, cx);
        gate.login_after_activation = login_after_activation;
        gate
    });
    Root::new(gate, window, cx)
}

#[cfg(feature = "e2e-test-support")]
pub fn build_activation_root_e2e(window: &mut Window, cx: &mut Context<Root>) -> Root {
    ui::init_components(cx);
    build_gated_root(BootPhase::Activation, false, window, cx)
}

#[cfg(feature = "e2e-test-support")]
pub fn build_configured_unlicensed_root_e2e(window: &mut Window, cx: &mut Context<Root>) -> Root {
    ui::init_components(cx);
    let missing_status = license::LocalLicenseStatus::Invalid {
        reason: license::LicenseInvalidReason::Missing,
    };
    let phase = super::super::boot::decide_boot_phase(
        super::super::config::LICENSES_ENABLED,
        false,
        &missing_status,
    );
    build_gated_root(phase, false, window, cx)
}

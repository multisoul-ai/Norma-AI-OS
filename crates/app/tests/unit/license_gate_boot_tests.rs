use super::boot::{decide_boot_phase, BootPhase};
use license::{LicenseInvalidReason, LocalLicenseStatus};

/// License 功能开关关闭：即使本地凭证缺失，也必须直接进入工作台。
///
/// 数据构造（含关键数值的推导过程）：
///   licenses_enabled = false（构建时显式关闭 Cargo feature `licenses`）
///   skip_license     = false（未使用 `NORMA_SKIP_LICENSE` 逃生开关）
///   local status     = Invalid(Missing)（模拟首次安装且没有离线凭证）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动决策先读取 licenses 功能开关 → 关闭
///   2. 功能关闭优先于本地凭证状态 → 直接选择 Workbench
///
/// 预期结果：
///   - 正断言：启动阶段为 Workbench
///   - 负断言：启动阶段不是 Activation，用户不会看到 License 激活页
#[test]
fn test_disabled_license_gate_forces_workbench_even_when_invalid() {
    let status = LocalLicenseStatus::Invalid {
        reason: LicenseInvalidReason::Missing,
    };
    let phase = decide_boot_phase(false, false, &status);

    assert_eq!(
        phase,
        BootPhase::Workbench,
        "licenses 功能开关关闭时必须直接进入工作台"
    );
    assert_ne!(
        phase,
        BootPhase::Activation,
        "licenses 功能开关关闭时不得进入 License 激活页"
    );
}

/// License 功能开启但 skip=true：无效凭证仍必须进入工作台。
///
/// 数据构造（含关键数值的推导过程）：
///   licenses_enabled = true（启用 License 硬门控）
///   skip_license     = true（启用开发逃生开关）
///   local status     = Invalid(Missing)（本地没有离线凭证）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动决策确认 licenses 功能已开启
///   2. skip_license 覆盖无效凭证状态 → 选择 Workbench
///
/// 预期结果：
///   - 正断言：启动阶段为 Workbench
///   - 负断言：启动阶段不是 Activation
#[test]
fn test_skip_forces_workbench_even_when_invalid() {
    let status = LocalLicenseStatus::Invalid {
        reason: LicenseInvalidReason::Missing,
    };
    let phase = decide_boot_phase(true, true, &status);

    assert_eq!(
        phase,
        BootPhase::Workbench,
        "NORMA_SKIP_LICENSE 必须直接进入工作台"
    );
    assert_ne!(
        phase,
        BootPhase::Activation,
        "NORMA_SKIP_LICENSE 启用时不得进入 License 激活页"
    );
}

/// License 功能开启且本地凭证有效：未使用 skip 也必须进入工作台。
///
/// 数据构造（含关键数值的推导过程）：
///   licenses_enabled = true（启用 License 硬门控）
///   skip_license     = false（不使用开发逃生开关）
///   license_id       = "lic-1"（已激活凭证的稳定标识）
///   exp              = 9_999_999_999（远大于当前 Unix 时间，模拟未过期）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动决策确认 licenses 功能已开启且未 skip
///   2. 本地状态为 Activated → 选择 Workbench
///
/// 预期结果：
///   - 正断言：启动阶段为 Workbench
///   - 负断言：启动阶段不是 Activation
#[test]
fn test_activated_without_skip_opens_workbench() {
    let status = LocalLicenseStatus::Activated {
        license_id: Some("lic-1".into()),
        exp: Some(9_999_999_999),
    };
    let phase = decide_boot_phase(true, false, &status);

    assert_eq!(phase, BootPhase::Workbench, "已激活应进入工作台");
    assert_ne!(
        phase,
        BootPhase::Activation,
        "有效本地凭证不得进入 License 激活页"
    );
}

/// License 功能开启且未 skip：每一种无效凭证都必须进入激活页。
///
/// 数据构造（含关键数值的推导过程）：
///   licenses_enabled = true（启用 License 硬门控）
///   skip_license     = false（不使用开发逃生开关）
///   invalid reasons  = Missing / Expired / DidMismatch / BadSignature
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别构造四种明确的无效本地凭证状态
///   2. 每次启动决策都执行有效性分支 → 选择 Activation
///
/// 预期结果：
///   - 正断言：四种无效原因都分别进入 Activation
///   - 负断言：四种无效原因都不得进入 Workbench
#[test]
fn test_invalid_without_skip_opens_activation() {
    let missing = decide_boot_phase(
        true,
        false,
        &LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::Missing,
        },
    );
    let expired = decide_boot_phase(
        true,
        false,
        &LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::Expired,
        },
    );
    let did_mismatch = decide_boot_phase(
        true,
        false,
        &LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::DidMismatch,
        },
    );
    let bad_signature = decide_boot_phase(
        true,
        false,
        &LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::BadSignature,
        },
    );

    assert_eq!(
        missing,
        BootPhase::Activation,
        "缺失凭证必须进入 License 激活页"
    );
    assert_ne!(
        missing,
        BootPhase::Workbench,
        "缺失凭证不得绕过已启用的 License 硬门控"
    );
    assert_eq!(
        expired,
        BootPhase::Activation,
        "过期凭证必须进入 License 激活页"
    );
    assert_ne!(
        expired,
        BootPhase::Workbench,
        "过期凭证不得绕过已启用的 License 硬门控"
    );
    assert_eq!(
        did_mismatch,
        BootPhase::Activation,
        "设备不匹配的凭证必须进入 License 激活页"
    );
    assert_ne!(
        did_mismatch,
        BootPhase::Workbench,
        "设备不匹配的凭证不得绕过已启用的 License 硬门控"
    );
    assert_eq!(
        bad_signature,
        BootPhase::Activation,
        "签名无效的凭证必须进入 License 激活页"
    );
    assert_ne!(
        bad_signature,
        BootPhase::Workbench,
        "签名无效的凭证不得绕过已启用的 License 硬门控"
    );
}

/// 登录页叠加在 License 决策之上：只有 License 已放行且需要登录时才进入登录页。
///
/// 数据构造：license_phase ∈ {Activation, Workbench} × show_login ∈ {true, false} 共 4 种组合。
/// 预期结果：
///   - 正断言：(Workbench, true) → Login；(Workbench, false) → Workbench
///   - 负断言：License 未放行（Activation）时无论是否需要登录都保持 Activation（先激活、后登录）
#[test]
fn test_login_phase_layers_on_top_of_license_phase() {
    use super::boot::{decide_phase_with_login, BootPhase};
    assert_eq!(
        decide_phase_with_login(BootPhase::Workbench, true),
        BootPhase::Login,
        "License 放行且需要登录时应进入登录页"
    );
    assert_eq!(
        decide_phase_with_login(BootPhase::Workbench, false),
        BootPhase::Workbench,
        "不需要登录时应直接进入工作台"
    );
    for show_login in [true, false] {
        assert_eq!(
            decide_phase_with_login(BootPhase::Activation, show_login),
            BootPhase::Activation,
            "License 未放行时必须先激活，show_login={show_login}"
        );
    }
}

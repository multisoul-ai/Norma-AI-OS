use super::boot::{decide_boot_phase, BootPhase};
use license::{LicenseInvalidReason, LocalLicenseStatus};

/// skip=true 时无论评估结果如何都进工作台。
///
/// 数据：Invalid(Missing) + skip true
/// 预期：Workbench（开发跳过硬门控）
#[test]
fn test_skip_forces_workbench_even_when_invalid() {
    let status = LocalLicenseStatus::Invalid {
        reason: LicenseInvalidReason::Missing,
    };
    assert_eq!(
        decide_boot_phase(true, &status),
        BootPhase::Workbench,
        "NORMA_SKIP_LICENSE 必须直接进入工作台"
    );
}

/// 有效本地凭证且未 skip → 工作台。
#[test]
fn test_activated_without_skip_opens_workbench() {
    let status = LocalLicenseStatus::Activated {
        license_id: Some("lic-1".into()),
        exp: Some(9_999_999_999),
    };
    assert_eq!(
        decide_boot_phase(false, &status),
        BootPhase::Workbench,
        "已激活应进入工作台"
    );
}

/// 无效凭证且未 skip → 激活壳。
#[test]
fn test_invalid_without_skip_opens_activation() {
    for reason in [
        LicenseInvalidReason::Missing,
        LicenseInvalidReason::Expired,
        LicenseInvalidReason::DidMismatch,
        LicenseInvalidReason::BadSignature,
    ] {
        let status = LocalLicenseStatus::Invalid { reason };
        assert_eq!(
            decide_boot_phase(false, &status),
            BootPhase::Activation,
            "无效原因 {reason:?} 应停留在激活壳"
        );
    }
}

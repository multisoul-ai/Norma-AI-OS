use super::*;
use ui::i18n::Locale;

fn integration(code: &str, message: &str) -> ProviderIntegration {
    let ready = || ComponentState::Ready {
        version: "test".into(),
    };
    ProviderIntegration::from_components(
        OrchestrationProvider::Claude,
        ready(),
        ready(),
        ready(),
        ComponentState::Failed {
            error_code: code.into(),
            message: message.into(),
        },
    )
}

/// 场景：结构化凭据权限错误在中英文设置页显示对应语言，原诊断保留在领域状态中。
/// 构造：同一个ProviderIntegration携带稳定error_code与原始英文诊断；不修改进程全局语言。
/// 执行：在真实设置行格式化边界分别投影两个Locale，再验证未知诊断保持原文。
/// 预期：中文不能嵌入英文产品提示，英文正常；原状态与用户/OS诊断均不得被覆盖。
#[test]
fn credential_permission_detail_is_localized_without_rewriting_diagnostics() {
    let raw = "Provider credential permissions must be restricted to its owner.";
    let state = integration("credential_permissions_insecure", raw);
    let zh = component_error_detail_for(&state, Locale::ZhCn).unwrap();
    let en = component_error_detail_for(&state, Locale::En).unwrap();
    assert_eq!(
        zh, "授权：Provider 凭据仅允许其所有者访问。 请选择“修复”重试。",
        "中文设置提示必须完全本地化"
    );
    assert_eq!(en, "Authorization: Provider credentials must be accessible only to their owner. Choose Repair to try again.", "英文设置提示必须保持自然完整");
    assert!(!zh.contains(raw), "中文不能直接插入后端英文产品提示");
    assert_eq!(
        state.authorization_component,
        integration("credential_permissions_insecure", raw).authorization_component,
        "展示翻译不得篡改结构化状态或原始诊断"
    );
    for locale in [Locale::ZhCn, Locale::En] {
        let detail =
            component_error_detail_for(&integration("unknown_error", "EACCES /用户/原文"), locale)
                .unwrap();
        assert!(
            detail.contains("EACCES /用户/原文"),
            "未知OS诊断及用户路径必须保留原文"
        );
    }
}

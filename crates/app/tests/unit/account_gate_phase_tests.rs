use super::gate_phase::{login_enabled_from, parse_skip_login, should_show_login, LoginGateInputs};

fn inputs(enabled: bool, skip: bool, has_session: bool, dismissed: bool) -> LoginGateInputs {
    LoginGateInputs {
        enabled,
        skip,
        has_session,
        dismissed,
    }
}

/// 只有「功能开启、未跳过、无会话、未点过稍后再说」时才显示启动登录页。
///
/// 数据构造：四个布尔输入共 2^4 = 16 种组合，只有 (true,false,false,false) 一种应显示。
/// 执行过程：遍历 16 种组合，逐一调用 `should_show_login`。
/// 预期结果：
///   - 正断言：唯一显示的组合是 enabled=true, skip=false, has_session=false, dismissed=false
///   - 负断言：其余 15 种组合全部不显示
#[test]
fn test_login_prompt_only_when_enabled_and_unseen() {
    let mut shown = Vec::new();
    for bits in 0..16u8 {
        let case = inputs(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
        if should_show_login(case) {
            shown.push((case.enabled, case.skip, case.has_session, case.dismissed));
        }
    }
    assert_eq!(
        shown,
        vec![(true, false, false, false)],
        "16 种组合中只有「开启、未跳过、无会话、未跳过标记」应显示登录页"
    );
}

/// NORMA_SKIP_LOGIN 的取值解析：与 License 跳过开关一致的真值集合。
///
/// 预期结果：正断言："1"、"true"、"on" 为真；负断言：缺失、"0"、空串为假。
#[test]
fn test_skip_login_env_parsing() {
    assert!(parse_skip_login(Some("1".into())), "1 应为真");
    assert!(
        parse_skip_login(Some(" true ".into())),
        "带空格的 true 应为真"
    );
    assert!(parse_skip_login(Some("on".into())), "on 应为真");
    assert!(!parse_skip_login(None), "缺失应为假");
    assert!(!parse_skip_login(Some("0".into())), "0 应为假");
    assert!(!parse_skip_login(Some(String::new())), "空串应为假");
}

/// 发布开关：feature 开启则始终启用；否则仅调试构建可用 NORMA_ACCOUNT_LOGIN 运行时开启。
///
/// 数据构造：feature ∈ {开, 关} × 调试构建 ∈ {是, 否} × 环境变量 ∈ {缺失, "1"} 共 8 种组合。
/// 预期结果：
///   - 正断言：feature 开启时全部启用；feature 关闭时只有「调试构建且环境变量为 1」启用
///   - 负断言：发行构建（非调试）下环境变量不能开启功能，避免终端用户误开未就绪的登录
#[test]
fn test_login_enabled_matrix() {
    for debug in [true, false] {
        for env in [None, Some("1".to_string())] {
            assert!(
                login_enabled_from(true, debug, env.clone()),
                "feature 开启时必须启用，debug={debug}, env={env:?}"
            );
        }
    }
    assert!(
        login_enabled_from(false, true, Some("1".into())),
        "调试构建 + 环境变量 1 应启用"
    );
    assert!(
        !login_enabled_from(false, true, None),
        "调试构建缺环境变量不应启用"
    );
    assert!(
        !login_enabled_from(false, false, Some("1".into())),
        "发行构建不得被环境变量开启"
    );
    assert!(
        !login_enabled_from(false, false, None),
        "默认发行构建必须关闭"
    );
}

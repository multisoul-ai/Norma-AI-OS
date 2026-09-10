use super::*;

/// 保存的通知服务按安装隔离；错误修改不覆盖凭据，停用不会回退到启动环境。
///
/// 数据构造：两个隔离 AppDB；32字节测试密钥编码为64位hex；HTTPS正式入口与不合法HTTP入口。
/// 执行过程：保存A → 重读A与B → 拒绝非法修改 → 清除A → 重读显式停用标记。
/// 预期结果：A恢复准确地址/环境/密钥，B未配置；非法修改保留A；停用后不残留密钥。
#[test]
fn saved_relay_is_isolated_validated_and_explicitly_disabled() {
    use super::super::configuration;
    let dir = tempfile::tempdir().expect("隔离目录");
    let a = storage::AppStore::new(dir.path().join("a.sqlite"));
    let b = storage::AppStore::new(dir.path().join("b.sqlite"));
    let key = "b".repeat(64); // 32 bytes × 2 hex digits = 64.
    let config = PushRelayConfig::from_fields(
        "https://push.example/v1/notifications",
        &key,
        PushEnvironment::Sandbox,
    )
    .expect("合法输入");
    configuration::save(&a, Some(&config)).expect("保存");
    let loaded = configuration::load(&a)
        .expect("重读")
        .expect("已设置")
        .expect("已配置");
    assert_eq!(
        loaded.endpoint.as_str(),
        "https://push.example/v1/notifications",
        "地址必须完整恢复"
    );
    assert_eq!(loaded.key, key, "密钥必须恢复且不能改写");
    assert_eq!(
        loaded.environment,
        PushEnvironment::Sandbox,
        "不能跨APNs环境"
    );
    assert!(
        configuration::load(&b).expect("B").is_none(),
        "另一安装不继承A配置"
    );
    assert!(
        PushRelayConfig::from_fields(
            "http://push.example/v1/notifications",
            &key,
            PushEnvironment::Sandbox
        )
        .is_err(),
        "不合法修改在写入前拒绝"
    );
    assert!(
        configuration::load(&a).expect("A").flatten().is_some(),
        "拒绝修改不删除原配置"
    );
    configuration::save(&a, None).expect("清除与停用");
    assert!(
        matches!(configuration::load(&a).expect("停用"), Some(None)),
        "停用必须显式保存，不能因环境变量复活"
    );
}

/// 通知服务只能连接明确HTTPS入口，不能通过地址或重定向把relay凭据发到其他目标。
///
/// 数据构造：64位hex relay key；正式https入口、外部http、地址内密码/查询/错误路径及换行凭据。
/// 执行过程：构造正式配置 → 尝试不可信地址和凭据 → 验证HTTP例外仅允许测试回环。
/// 预期结果：标准HTTPS成功；明文外部地址/含身份地址/不合法key全部失败。
#[test]
fn relay_configuration_preserves_the_https_credential_boundary() {
    let key = "a".repeat(64); // 32 random bytes = 64 hex chars in deployment; fixed test-only value.
    assert!(
        PushRelayConfig::new(
            "https://push.example/v1/notifications",
            &key,
            PushEnvironment::Sandbox,
            false
        )
        .is_ok(),
        "正式HTTPS入口应该可配置"
    );
    assert!(
        PushRelayConfig::new(
            "http://push.example/v1/notifications",
            &key,
            PushEnvironment::Sandbox,
            false
        )
        .is_err(),
        "正式配置不能把凭据发送到明文HTTP"
    );
    assert!(
        PushRelayConfig::new(
            "http://127.0.0.1/v1/notifications",
            &key,
            PushEnvironment::Sandbox,
            false
        )
        .is_err(),
        "正式模式也不能绕过HTTPS"
    );
    assert!(
        PushRelayConfig::new(
            "https://user:password@push.example/v1/notifications",
            &key,
            PushEnvironment::Sandbox,
            false
        )
        .is_err(),
        "地址不能内嵌另一套认证"
    );
    assert!(
        PushRelayConfig::new(
            "https://push.example/v1/notifications?token=secret",
            &key,
            PushEnvironment::Sandbox,
            false
        )
        .is_err(),
        "查询不能泄漏凭据或改变入口语义"
    );
    assert!(
        PushRelayConfig::new(
            "https://push.example/other",
            &key,
            PushEnvironment::Sandbox,
            false
        )
        .is_err(),
        "只能投递到明确通知路径"
    );
    assert!(
        PushRelayConfig::new(
            "https://push.example/v1/notifications",
            "abc\nHeader: x",
            PushEnvironment::Sandbox,
            false
        )
        .is_err(),
        "relay凭据不能注入Header"
    );
    assert!(
        PushRelayConfig::new(
            "http://127.0.0.1/v1/notifications",
            &key,
            PushEnvironment::Sandbox,
            true
        )
        .is_ok(),
        "隔离测试只允许字面回环HTTP"
    );
    assert!(
        PushRelayConfig::new(
            "http://192.168.1.1/v1/notifications",
            &key,
            PushEnvironment::Sandbox,
            true
        )
        .is_err(),
        "测试开关不能访问局域网明文服务"
    );
}

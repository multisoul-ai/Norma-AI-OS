use super::*;

/// Relay 明确拒绝请求时保留可诊断原因，不能把所有失败都当成未知投递。
///
/// 数据构造：本地 HTTP 返回协议 400/401/502/503，另给出含敏感文本的未知错误和伪造成功；密钥 32 bytes × 2 = 64 hex。
/// 执行过程：每例接收一次真实 POST → 返回固定状态及正文 → 检查 Mac 的结果分类。
/// 预期结果：非法事件、APNs 配置和服务不可用分别可辨；未知正文不进入错误码，伪造成功不算接受。
#[test]
fn relay_preserves_known_rejections_without_exposing_response_contents() {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    let send = |status: u16, body: &'static str| {
        let listener = TcpListener::bind("127.0.0.1:0").expect("隔离 HTTP 入口");
        let endpoint = format!(
            "http://{}/v1/notifications",
            listener.local_addr().expect("地址")
        );
        let server = std::thread::spawn(move || {
            let (socket, _) = listener.accept().expect("接收唯一请求");
            // 5 秒为本地 IO 预算，小于生产请求的 12 秒总预算。
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("读取上限");
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            let mut length = 0;
            loop {
                line.clear();
                assert!(
                    reader.read_line(&mut line).expect("请求头") > 0,
                    "请求头不能提前结束"
                );
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse::<usize>().expect("请求长度");
                    }
                }
            }
            let mut request = vec![0; length];
            reader.read_exact(&mut request).expect("完整请求");
            assert_eq!(request, b"{}", "本地探针不含真实设备或任务");
            write!(reader.get_mut(), "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("响应");
        });
        let config = PushRelayConfig::new(
            &endpoint,
            &"a".repeat(64),
            PushEnvironment::Production,
            true,
        )
        .expect("本地配置");
        let mut relay = Relay::new(config);
        // 沿用生产总预算：APNs 10 秒 + 网络余量 2 秒 = 12 秒；仅禁用测试进程代理。
        relay.agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(12)))
            .proxy(None)
            .build()
            .new_agent();
        let outcome = relay.send(&serde_json::json!({}));
        server.join().expect("HTTP 服务退出");
        outcome
    };
    assert_eq!(
        send(400, r#"{"error":"invalid_event"}"#),
        Err("invalid_event"),
        "协议拒绝不能伪装为未知投递"
    );
    assert_eq!(
        send(401, "Unauthorized"),
        Err("push_configuration_error"),
        "代理返回非 JSON 的认证拒绝仍须指向配置问题"
    );
    assert_eq!(
        send(502, r#"{"error":"apns_configuration_error"}"#),
        Err("apns_configuration_error"),
        "Apple 密钥错误必须可定位"
    );
    assert_eq!(
        send(503, r#"{"error":"apns_unavailable"}"#),
        Err("apns_unavailable"),
        "Apple 暂不可用必须可辨"
    );
    assert_eq!(
        send(502, r#"{"error":"SECRET_RESPONSE"}"#),
        Err("delivery_unknown"),
        "任意响应正文不得进入日志错误码"
    );
    assert_eq!(
        send(200, r#"{"status":"delivered"}"#),
        Err("delivery_unknown"),
        "非 accepted 回执不能报成功"
    );
    assert_eq!(
        send(422, r#"{"error":"token_invalid"}"#),
        Ok(Some(0)),
        "错误topic/token必须失效准确订阅，不能继续投递"
    );
    assert_eq!(
        send(200, r#"{"status":"accepted"}"#),
        Ok(None),
        "合法 APNs 接受回执必须保持成功"
    );
}

/// 保存的通知服务按安装隔离；错误修改不覆盖凭据，停用不会回退到启动环境。
///
/// 数据构造：两个隔离 AppDB；32字节测试密钥编码为64位hex；主域名 /norma/ 下的 HTTPS 入口与不合法 HTTP 入口。
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
        "https://soulvoiceai.com/norma/v1/notifications",
        &key,
        PushEnvironment::Sandbox,
    )
    .expect("合法输入");
    configuration::save(&a, Some(&config)).expect("保存");
    let persisted = a
        .settings_store()
        .get_string("mobile.push.config")
        .expect("应能读取 SQLite 推送配置");
    assert!(
        persisted
            .as_deref()
            .is_some_and(|value| value.contains(&key)),
        "推送凭据必须写入隔离 app.sqlite"
    );
    let loaded = configuration::load(&a)
        .expect("重读")
        .expect("已设置")
        .expect("已配置");
    assert_eq!(
        loaded.endpoint.as_str(),
        "https://soulvoiceai.com/norma/v1/notifications",
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
    let disabled = a
        .settings_store()
        .get_string("mobile.push.config")
        .expect("应能读取 SQLite 停用标记");
    assert_eq!(
        disabled.as_deref(),
        Some("null"),
        "停用必须持久化明确的 null 标记"
    );
    assert!(
        !disabled.as_deref().unwrap_or_default().contains(&key),
        "停用后 SQLite 不得残留推送密钥"
    );
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
        PushRelayConfig::from_fields(
            "https://soulvoiceai.com/norma/other",
            &key,
            PushEnvironment::Production
        )
        .is_err(),
        "主域名下非通知路径不能收到服务凭据"
    );
    assert!(
        PushRelayConfig::from_fields(
            "https://soulvoiceai.com/norma/v1/notifications/extra",
            &key,
            PushEnvironment::Production
        )
        .is_err(),
        "通知入口后不能追加其他路由"
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

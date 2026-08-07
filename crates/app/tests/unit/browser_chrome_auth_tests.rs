use super::chrome_auth::{parse_cookie_export, ChromeAuthRequest, SameSitePolicy};

/// CA-1：匹配当前 Browser URL 的 Chrome 导出应保留可移植的会话 Cookie 属性。
///
/// 数据构造（含关键数值的推导过程）：
///   protocol_version = 1，当前协议唯一支持版本；
///   request_id = 7、node_id = 42，用于把异步响应绑定回唯一请求和卡片；
///   now = 1_900_000_000，expiry = 2_000_000_000，因此剩余有效期
///         = 2_000_000_000 - 1_900_000_000 = 100_000_000 秒；
///   cookies = 1 条，低于单次导入上限。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为 `https://accounts.example.test/dashboard` 构造 URL-scoped 请求。
///   2. 解析扩展返回的同 request/node/url JSON。
///   3. 校验 Cookie 未过期、未分区，转换为 native Browser 可消费的结构。
///
/// 预期结果：
///   - 正断言：导入批次恰有 1 条 Cookie，且 name/domain/path/value 精确保留。
///   - 正断言：HttpOnly、Secure、SameSite=Lax 和 expiry 精确保留。
///   - 负断言：skipped 必须为 0，合法 Cookie 不得被静默丢弃。
#[test]
fn test_url_scoped_cookie_export_preserves_supported_auth_attributes() {
    let request = ChromeAuthRequest::new(7, 42, "https://accounts.example.test/dashboard")
        .expect("有效 HTTP(S) Browser URL 必须能创建 Chrome auth 请求");
    let response = br#"{
        "version": 1,
        "type": "cookie_export",
        "request_id": 7,
        "node_id": 42,
        "url": "https://accounts.example.test/dashboard",
        "cookies": [{
            "name": "session_id",
            "value": "opaque-session-token",
            "domain": ".example.test",
            "hostOnly": false,
            "path": "/",
            "secure": true,
            "httpOnly": true,
            "sameSite": "lax",
            "session": false,
            "expirationDate": 2000000000,
            "storeId": "0"
        }]
    }"#;

    let batch = parse_cookie_export(response, &request, 1_900_000_000)
        .expect("匹配请求的合法 Chrome Cookie export 必须通过验证");
    assert_eq!(
        batch.cookies.len(),
        1,
        "合法导出应产生恰好 1 条可导入 Cookie"
    );
    let cookie = &batch.cookies[0];
    assert_eq!(cookie.name, "session_id", "Cookie name 必须精确保留");
    assert_eq!(
        cookie.value, "opaque-session-token",
        "Cookie value 必须在内存转换中精确保留"
    );
    assert_eq!(
        cookie.domain, ".example.test",
        "domain cookie 的前导点必须保留，不能误变为 host-only"
    );
    assert_eq!(cookie.path, "/", "Cookie path 必须精确保留");
    assert!(
        !cookie.host_only,
        "Chrome domain Cookie 不得在转换中误标为 host-only"
    );
    assert!(cookie.secure, "Secure Cookie 导入后必须继续限制为安全传输");
    assert!(cookie.http_only, "HttpOnly Cookie 导入后不得降级为脚本可读");
    assert_eq!(
        cookie.same_site,
        SameSitePolicy::Lax,
        "SameSite=Lax 必须精确保留"
    );
    assert_eq!(
        cookie.expires_unix_seconds,
        Some(2_000_000_000),
        "持久 Cookie 的绝对过期时间必须精确保留"
    );
    assert_eq!(batch.skipped, 0, "合法且可移植的 Cookie 不应被跳过");
}

/// CA-2：Chrome 导出只能导入当前 URL 可用的未过期、未分区 Cookie。
///
/// 数据构造（含关键数值的推导过程）：
///   request_host = accounts.example.test；
///   cookies = 4 条：同 host session 1 条、evil.test 跨域 1 条、过期 1 条、分区 1 条；
///   now = 1_900_000_000，expired = 1_800_000_000，已过期
///         = 1_900_000_000 - 1_800_000_000 = 100_000_000 秒；
///   expected_imported = 4 - 3（跨域/过期/分区）= 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析匹配当前 Browser 请求的 4 条 Cookie 导出。
///   2. 逐条校验 domain scope、expiry 与 partitionKey。
///   3. 只输出当前 host 的普通 session Cookie，并累计跳过数量。
///
/// 预期结果：
///   - 正断言：只保留 `host_session`，且它仍是 session Cookie。
///   - 正断言：skipped 精确为 3。
///   - 负断言：跨域、过期、分区 Cookie name 均不在结果中。
///   - 负断言：Debug 输出不得包含任何 Cookie value。
#[test]
fn test_cookie_export_rejects_cross_domain_expired_and_partitioned_credentials() {
    let request = ChromeAuthRequest::new(11, 84, "https://accounts.example.test/dashboard")
        .expect("有效 HTTPS Browser URL 必须能创建 Chrome auth 请求");
    let response = br#"{
        "version": 1,
        "type": "cookie_export",
        "request_id": 11,
        "node_id": 84,
        "url": "https://accounts.example.test/dashboard",
        "cookies": [
            {
                "name": "host_session", "value": "allowed-secret",
                "domain": "accounts.example.test", "hostOnly": true, "path": "/",
                "secure": true, "httpOnly": true, "sameSite": "strict",
                "session": true, "storeId": "0"
            },
            {
                "name": "cross_domain", "value": "evil-secret",
                "domain": ".evil.test", "hostOnly": false, "path": "/",
                "secure": true, "httpOnly": true, "sameSite": "lax",
                "session": true, "storeId": "0"
            },
            {
                "name": "expired_session", "value": "expired-secret",
                "domain": ".example.test", "hostOnly": false, "path": "/",
                "secure": true, "httpOnly": true, "sameSite": "lax",
                "session": false, "expirationDate": 1800000000, "storeId": "0"
            },
            {
                "name": "partitioned_session", "value": "partitioned-secret",
                "domain": ".example.test", "hostOnly": false, "path": "/",
                "secure": true, "httpOnly": true, "sameSite": "no_restriction",
                "session": true, "storeId": "0",
                "partitionKey": {"topLevelSite": "https://container.test"}
            }
        ]
    }"#;

    let batch = parse_cookie_export(response, &request, 1_900_000_000)
        .expect("混合导出应跳过不安全 Cookie，而不是让整次合法响应失败");
    assert_eq!(
        batch.cookies.len(),
        1,
        "4 条中只有当前 host 的普通未过期 Cookie 可导入"
    );
    assert_eq!(
        batch.cookies[0].name, "host_session",
        "导入结果必须精确保留当前 host session Cookie"
    );
    assert_eq!(
        batch.cookies[0].expires_unix_seconds, None,
        "Chrome session Cookie 不应被伪造成持久 Cookie"
    );
    assert!(
        batch.cookies[0].host_only,
        "Chrome host-only Cookie 必须保持 host-only，不能扩大到 sibling subdomain"
    );
    assert_eq!(
        batch.skipped, 3,
        "跨域、过期、分区 Cookie 必须各计为 1 条 skipped"
    );
    let imported_names = batch
        .cookies
        .iter()
        .map(|cookie| cookie.name.as_str())
        .collect::<Vec<_>>();
    assert!(
        !imported_names.contains(&"cross_domain"),
        "evil.test Cookie 不得导入 accounts.example.test"
    );
    assert!(
        !imported_names.contains(&"expired_session"),
        "已过期认证 Cookie 不得重新激活"
    );
    assert!(
        !imported_names.contains(&"partitioned_session"),
        "无法保留 partitionKey 时不得把分区 Cookie 降级为非分区 Cookie"
    );
    let debug = format!("{batch:?}");
    assert!(
        debug.contains("[REDACTED]"),
        "Cookie batch Debug 必须明确遮蔽 value"
    );
    assert!(
        !debug.contains("allowed-secret")
            && !debug.contains("evil-secret")
            && !debug.contains("expired-secret")
            && !debug.contains("partitioned-secret"),
        "Cookie batch Debug 不得泄露任何导出 value，actual={debug}"
    );
}

/// CA-3：Browser-neutral Cookie 必须无损转换为 Wry/WKWebView Cookie。
///
/// 数据构造（含关键数值的推导过程）：
///   expiry = 2_000_000_000 Unix seconds，落在 i64 与 Wry time 支持范围；
///   attributes = domain/path + Secure + HttpOnly + SameSite=Strict；
///   cookie_count = 1，验证一次单 Cookie backend 边界。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造已通过 Chrome 协议验证的 BrowserCookie。
///   2. 通过 Wry backend conversion seam 生成 `wry::cookie::Cookie`。
///   3. 从 Wry public accessors 读取所有授权相关属性。
///
/// 预期结果：
///   - 正断言：name/value/path 精确保留，domain 按 cookie crate 规范去除前导点。
///   - 正断言：Secure、HttpOnly、SameSite=Strict、expiry 精确保留。
///   - 负断言：持久 Cookie 不得退化成 session Cookie。
#[test]
fn test_wry_backend_cookie_conversion_preserves_auth_scope_and_expiry() {
    let source = super::chrome_auth::BrowserCookie {
        name: "session_id".to_string(),
        value: "opaque-session-token".to_string(),
        domain: ".example.test".to_string(),
        host_only: false,
        path: "/account".to_string(),
        secure: true,
        http_only: true,
        same_site: SameSitePolicy::Strict,
        expires_unix_seconds: Some(2_000_000_000),
    };

    let cookie = super::wry_backend::wry_cookie_from_browser_cookie(&source)
        .expect("已验证且 expiry 可表示的 BrowserCookie 必须能转换为 Wry Cookie");
    assert_eq!(cookie.name(), "session_id", "Wry Cookie name 必须精确保留");
    assert_eq!(
        cookie.value(),
        "opaque-session-token",
        "Wry Cookie value 必须精确保留"
    );
    assert_eq!(
        cookie.domain(),
        Some("example.test"),
        "Wry cookie crate 应只规范化 domain 前导点，不得改变 example.test scope"
    );
    assert_eq!(
        cookie.path(),
        Some("/account"),
        "Wry Cookie path 必须精确保留"
    );
    assert_eq!(
        cookie.secure(),
        Some(true),
        "Wry Cookie 必须继续标记 Secure"
    );
    assert_eq!(
        cookie.http_only(),
        Some(true),
        "Wry Cookie 必须继续标记 HttpOnly"
    );
    assert_eq!(
        cookie.same_site(),
        Some(wry::cookie::SameSite::Strict),
        "Wry Cookie 必须继续标记 SameSite=Strict"
    );
    assert_eq!(
        cookie
            .expires_datetime()
            .map(|expires| expires.unix_timestamp()),
        Some(2_000_000_000),
        "Wry Cookie 必须精确保留 Unix expiry"
    );
    assert!(
        !cookie.expires().is_some_and(|expiry| expiry.is_session()),
        "持久 Chrome Cookie 不得退化成 Wry session Cookie"
    );
}

/// CA-4：Chrome auth 请求与响应必须同时绑定合法 HTTP(S) URL 和唯一请求身份。
///
/// 数据构造（含关键数值的推导过程）：
///   valid request_id = 21、node_id = 128；
///   mismatched response request_id = 22，因此 identity delta = 22 - 21 = 1；
///   invalid schemes = 2 个：`file:` 与 `chrome:`；
///   valid schemes = 2 个：`http:` 与 `https:`。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别创建 HTTP、HTTPS、file 与 chrome URL 请求。
///   2. 确认只接受带 host 的 HTTP(S)。
///   3. 用 request_id 相差 1 的响应攻击 pending request identity 校验。
///
/// 预期结果：
///   - 正断言：HTTP 与 HTTPS 请求均可创建。
///   - 负断言：file/chrome 请求均被拒绝。
///   - 负断言：request_id 不匹配的响应不得进入 Cookie 转换。
#[test]
fn test_auth_protocol_rejects_non_http_urls_and_mismatched_response_identity() {
    assert!(
        ChromeAuthRequest::new(21, 128, "http://example.test/path").is_ok(),
        "带 host 的 HTTP Browser URL 必须支持显式 Chrome auth"
    );
    let request = ChromeAuthRequest::new(21, 128, "https://example.test/path")
        .expect("带 host 的 HTTPS Browser URL 必须支持显式 Chrome auth");
    assert!(
        ChromeAuthRequest::new(21, 128, "file:///tmp/private.html").is_err(),
        "file URL 不得触发 Chrome Cookie 导出"
    );
    assert!(
        ChromeAuthRequest::new(21, 128, "chrome://settings").is_err(),
        "Chrome 内部 URL 不得进入 Browser auth Native Messaging 协议"
    );

    let mismatched = br#"{
        "version": 1,
        "type": "cookie_export",
        "request_id": 22,
        "node_id": 128,
        "url": "https://example.test/path",
        "cookies": []
    }"#;
    let error = parse_cookie_export(mismatched, &request, 1_900_000_000)
        .expect_err("request_id 不匹配的扩展响应必须被拒绝");
    assert!(
        error.contains("did not match"),
        "identity mismatch 应返回稳定的 redacted 类别，actual={error}"
    );
    assert!(
        !error.contains("https://example.test/path"),
        "identity mismatch 错误不得回显当前 Browser URL"
    );
}

/// CA-5：Chrome auth 响应必须同时受 1 MiB frame 与 512 Cookie 数量上限约束。
///
/// 数据构造（含关键数值的推导过程）：
///   max bytes = 1024 * 1024 = 1_048_576；oversized = max + 1 = 1_048_577；
///   max cookies = 512；oversized count = 512 + 1 = 513；
///   request_id = 31、node_id = 256，与 count fixture 身份精确匹配。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入 1_048_577-byte payload，确认 JSON parse 前即拒绝。
///   2. 构造合法 JSON 但包含 513 条同域 Cookie。
///   3. 确认 identity 校验后、逐 Cookie 转换前由 count guard 拒绝。
///
/// 预期结果：
///   - 正断言：两条错误分别命中 size 与 count safety limit。
///   - 负断言：超限输入不得返回空的成功导入批次。
#[test]
fn test_auth_protocol_rejects_oversized_payload_and_cookie_count() {
    let request = ChromeAuthRequest::new(31, 256, "https://example.test/path")
        .expect("测试用 HTTPS 请求必须合法");
    let oversized_payload = vec![b' '; 1024 * 1024 + 1];
    let size_error = parse_cookie_export(&oversized_payload, &request, 1_900_000_000)
        .expect_err("超过 1 MiB 的扩展响应必须在 JSON parse 前被拒绝");
    assert!(
        size_error.contains("1 MiB safety limit"),
        "oversized payload 必须命中 byte-size safety limit，actual={size_error}"
    );

    let cookies = (0..513)
        .map(|index| {
            serde_json::json!({
                "name": format!("session_{index}"),
                "value": "opaque",
                "domain": "example.test",
                "hostOnly": true,
                "path": "/",
                "secure": true,
                "httpOnly": true,
                "sameSite": "lax",
                "session": true,
                "storeId": "0"
            })
        })
        .collect::<Vec<_>>();
    let oversized_count = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "type": "cookie_export",
        "request_id": 31,
        "node_id": 256,
        "url": "https://example.test/path",
        "cookies": cookies
    }))
    .expect("513-Cookie fixture 必须能编码成合法协议 JSON");
    let count_error = parse_cookie_export(&oversized_count, &request, 1_900_000_000)
        .expect_err("超过 512 条 Cookie 的响应必须被拒绝");
    assert!(
        count_error.contains("cookie count safety limit"),
        "513 条 Cookie 必须命中 count safety limit，actual={count_error}"
    );
    assert_ne!(
        count_error, "",
        "超限响应不得伪装成空错误或成功的空导入批次"
    );
}

/// CA-6：CEF 转换必须用空 domain 保持 host-only，并保留普通 domain Cookie scope。
///
/// 数据构造（含关键数值的推导过程）：
///   Cookie count = 2：host-only `__Host-session` 与 domain `shared_session`；
///   host-only domain input = `accounts.example.test`；CEF expected domain = empty；
///   domain Cookie input/expected = `.example.test`；expiry = session，因此无时间换算。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别构造 host-only 与普通 domain BrowserCookie。
///   2. 通过 CEF conversion 使用的纯 domain seam 计算两个值。
///   3. 比较两种 scope 表达；不在未加载 CEF framework 的单测进程构造 CefString。
///
/// 预期结果：
///   - 正断言：host-only Cookie 的 CEF domain 为空，由 source URL 锁定 host。
///   - 正断言：普通 domain Cookie 继续保留 `.example.test`。
///   - 负断言：host-only Cookie 不得扩大成 `.example.test` domain Cookie。
#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
#[test]
fn test_cef_cookie_conversion_preserves_host_only_scope() {
    let host_only = super::chrome_auth::BrowserCookie {
        name: "__Host-session".to_string(),
        value: "host-secret".to_string(),
        domain: "accounts.example.test".to_string(),
        host_only: true,
        path: "/".to_string(),
        secure: true,
        http_only: true,
        same_site: SameSitePolicy::Strict,
        expires_unix_seconds: None,
    };
    let domain_cookie = super::chrome_auth::BrowserCookie {
        name: "shared_session".to_string(),
        value: "domain-secret".to_string(),
        domain: ".example.test".to_string(),
        host_only: false,
        path: "/".to_string(),
        secure: true,
        http_only: true,
        same_site: SameSitePolicy::Lax,
        expires_unix_seconds: None,
    };

    let cef_host_only = super::cef_backend::cef_cookie_domain(&host_only);
    let cef_domain = super::cef_backend::cef_cookie_domain(&domain_cookie);
    assert_eq!(
        cef_host_only, "",
        "CEF host-only Cookie 必须使用空 domain，由 set_cookie source URL 锁定 host"
    );
    assert_eq!(
        cef_domain, ".example.test",
        "CEF domain Cookie 必须保留原始 sibling-subdomain scope"
    );
    assert_ne!(
        cef_host_only, cef_domain,
        "CEF 转换不得把 host-only Cookie 扩大为普通 domain Cookie"
    );
}

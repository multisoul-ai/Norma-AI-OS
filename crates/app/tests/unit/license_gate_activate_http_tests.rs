use super::activate_http::{activate_license, ActivateHttpError};
use license::ActivateLicenseRequest;
use std::io::{Read, Write};
use std::net::TcpListener;

fn serve_keygate_error_once(
    status: u16,
    body: &'static str,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("测试 HTTP 服务应能绑定临时本地端口");
    let address = listener
        .local_addr()
        .expect("测试 HTTP 服务应能读取本地地址");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("测试 HTTP 服务应收到 activate 请求");
        let mut request = [0_u8; 4096];
        let _ = stream
            .read(&mut request)
            .expect("测试 HTTP 服务应能读取 activate 请求");
        let reason = if status == 404 { "Not Found" } else { "Error" };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("测试 HTTP 服务应能返回 Keygate 错误响应");
    });
    (format!("http://{address}"), server)
}

/// 错误 License Key：Keygate 返回 404 业务错误时不得显示为网络故障。
///
/// 数据构造（含关键数值的推导过程）：
///   HTTP status = 404（Keygate 对不存在 License Key 的实际响应）
///   error code  = LICENSE_NOT_FOUND
///   message     = "license not found"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 本地 HTTP 服务接收 activate 请求 → 返回 Keygate 格式的 404 JSON
///   2. activate_license 解析 HTTP 响应 → 将非 2xx 分类为 API 业务错误
///   3. 生成用户可见文本 → 保留服务端的 license not found 信息
///
/// 预期结果：
///   - 正断言：错误类型为 Api 且 status 为 404
///   - 正断言：用户可见文本为“激活失败：License Key 不存在”
///   - 负断言：用户可见文本不得包含 HTTP 404
///   - 负断言：用户可见文本不得包含 network error
#[test]
fn test_license_not_found_is_api_error_instead_of_network_error() {
    let body =
        r#"{"success":false,"error":{"code":"LICENSE_NOT_FOUND","message":"license not found"}}"#;
    let (base_url, server) = serve_keygate_error_once(404, body);
    let request = ActivateLicenseRequest {
        license_key: "KG-WRONG-KEY".into(),
        identifier: "test-device".into(),
        identifier_type: "device".into(),
        label: "norma-http-test".into(),
    };

    let error = activate_license(&base_url, &request)
        .expect_err("不存在的 License Key 必须返回错误，不得激活成功");
    server.join().expect("测试 HTTP 服务线程不应 panic");

    match &error {
        ActivateHttpError::Api { status, body } => {
            assert_eq!(
                *status, 404,
                "不存在的 License Key 必须保留 Keygate 的 HTTP 404 状态"
            );
            assert!(
                body.contains("license not found"),
                "API 错误必须保留 Keygate 返回的 license not found 正文"
            );
        }
        other => panic!("HTTP 404 必须分类为 Api 错误，实际为 {other:?}"),
    }

    let visible = error.to_string();
    assert_eq!(
        visible, "激活失败：License Key 不存在",
        "用户可见错误必须把 Keygate LICENSE_NOT_FOUND 翻译为明确的 License Key 不存在提示，且不暴露 HTTP 状态"
    );
    assert!(
        !visible.contains("HTTP 404"),
        "License Key 不存在属于业务错误，用户可见文案不得暴露 HTTP 404，实际为 {visible}"
    );
    assert!(
        !visible.contains("network error"),
        "服务器已返回 HTTP 404 时不得误报网络故障，实际为 {visible}"
    );
}

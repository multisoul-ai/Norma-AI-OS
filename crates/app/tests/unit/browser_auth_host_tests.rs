use super::{read_native_message, write_native_message, MAX_NATIVE_MESSAGE_BYTES};
use std::io::Cursor;
use std::path::Path;

/// CAH-1：Native Messaging frame 必须按 payload 字节数写入 4-byte native-endian 长度。
///
/// 数据构造（含关键数值的推导过程）：
///   payload = UTF-8 JSON `{"text":"登录"}`；两个汉字各占 3 bytes；
///   frame bytes = 4-byte length prefix + payload.len()；
///   payload.len() 小于 1 MiB = 1024 * 1024 bytes 上限。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将 payload 写入内存 writer。
///   2. 独立读取前 4 bytes 并按 native endian 解释长度。
///   3. 再通过 reader seam 解帧，验证 Unicode JSON 原样返回。
///
/// 预期结果：
///   - 正断言：前缀精确等于 UTF-8 payload 字节数，而不是字符数。
///   - 正断言：frame 总长精确等于 4 + payload.len()。
///   - 正断言：read/write round trip 精确保留 payload。
///   - 负断言：合法 frame 不得被误判为超过 1 MiB。
#[test]
fn test_native_message_frame_uses_byte_length_and_round_trips_unicode_json() {
    let payload = r#"{"text":"登录"}"#.as_bytes();
    assert!(
        payload.len() < MAX_NATIVE_MESSAGE_BYTES,
        "测试 payload 必须小于 1 MiB，才能验证合法 frame 路径"
    );

    let mut framed = Vec::new();
    write_native_message(&mut framed, payload)
        .expect("小于上限的 Native Messaging payload 必须可写入");
    let prefix = u32::from_ne_bytes(
        framed[0..4]
            .try_into()
            .expect("Native Messaging frame 必须至少包含 4-byte length prefix"),
    ) as usize;
    assert_eq!(
        prefix,
        payload.len(),
        "Native Messaging 长度必须使用 UTF-8 bytes，不得使用字符数"
    );
    assert_eq!(
        framed.len(),
        4 + payload.len(),
        "frame 总长必须精确等于 4-byte prefix + payload"
    );

    let decoded = read_native_message(&mut Cursor::new(framed))
        .expect("刚写出的合法 frame 必须能由同一协议 seam 解码");
    assert_eq!(
        decoded, payload,
        "Native Messaging read/write round trip 必须精确保留 Unicode JSON bytes"
    );
    assert_ne!(
        decoded.len(),
        MAX_NATIVE_MESSAGE_BYTES + 1,
        "合法 payload 不得被误扩展为超限消息"
    );
}

/// CAH-2：Chrome Native Messaging host manifest 必须只授权固定扩展 origin。
///
/// 数据构造（含关键数值的推导过程）：
///   extension_id = 32 个字符，由 manifest 公钥 SHA-256 前 16 bytes 的 32 个半字节推导；
///   allowed_origins = 1，只允许 Norma 官方扩展；
///   host path = `/Applications/Norma OS.app/Contents/MacOS/norma-browser-auth-host`，绝对路径。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用打包后的绝对 helper 路径生成 Native Messaging manifest JSON。
///   2. 解析 JSON 并读取 name/type/path/allowed_origins。
///   3. 检查 Chrome 用户级 manifest 安装路径。
///
/// 预期结果：
///   - 正断言：host name/type/path 与 Chrome 协议精确匹配。
///   - 正断言：allowed_origins 只有固定 `chrome-extension://.../`。
///   - 负断言：origin 不含通配符，其他扩展不能调用 helper。
///   - 正断言：安装目标位于 Google Chrome 的用户级 NativeMessagingHosts 目录。
#[test]
fn test_native_host_manifest_allows_only_bundled_chrome_extension() {
    let helper = Path::new("/Applications/Norma OS.app/Contents/MacOS/norma-browser-auth-host");
    let manifest = super::native_host_manifest(helper)
        .expect("绝对 helper 路径必须能生成 Chrome Native Messaging manifest");
    let json: serde_json::Value = serde_json::from_slice(&manifest)
        .expect("生成的 Native Messaging manifest 必须是合法 JSON");

    assert_eq!(
        json["name"], "com.norma.os.browser_auth",
        "Native Messaging host name 必须与扩展 connectNative 名称一致"
    );
    assert_eq!(
        json["type"], "stdio",
        "Chrome Native Messaging 只接受 stdio host"
    );
    assert_eq!(
        json["path"],
        helper.to_string_lossy().as_ref(),
        "macOS Native Messaging host path 必须是 helper 的绝对路径"
    );
    let origins = json["allowed_origins"]
        .as_array()
        .expect("allowed_origins 必须是 JSON array");
    assert_eq!(
        origins.len(),
        1,
        "Native host 必须只允许 1 个随包扩展 origin"
    );
    assert_eq!(
        origins[0], "chrome-extension://fkhlnglmgbebphicanlcckhpflbdfopf/",
        "Native host 必须只授权由固定 manifest key 推导出的扩展 ID"
    );
    assert!(
        !origins[0]
            .as_str()
            .expect("allowed origin 必须是 string")
            .contains('*'),
        "allowed_origins 禁止通配符，否则任意扩展都能请求认证 Cookie"
    );

    let install_path = super::chrome_native_host_manifest_path(Path::new("/Users/tester"));
    assert_eq!(
        install_path,
        Path::new("/Users/tester/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.norma.os.browser_auth.json"),
        "用户级 host manifest 必须安装到 Google Chrome 官方查找目录"
    );
}

/// CAH-3：扩展安装指引必须从运行中 `.app` 定位到随包 Resources 目录。
///
/// 数据构造（含关键数值的推导过程）：
///   executable depth = `Contents/MacOS/norma-os` 3 层；
///   resource sibling = 从 MacOS 上移 1 层到 Contents，再进入 Resources；
///   extension directory count = 1，固定为 `browser-auth-extension`。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入标准 macOS `.app` 主程序绝对路径。
///   2. 通过纯路径 seam 推导 bundled extension 目录。
///   3. 比较目标目录，并确认未错误落到 MacOS helper 目录。
///
/// 预期结果：
///   - 正断言：路径精确为 `Contents/Resources/browser-auth-extension`。
///   - 负断言：路径不得仍位于 `Contents/MacOS`。
#[test]
fn test_bundled_extension_path_is_resolved_from_app_resources() {
    let executable = Path::new("/Applications/Norma OS.app/Contents/MacOS/norma-os");
    let extension = super::bundled_extension_path_for_executable(executable)
        .expect("标准 `.app` executable 路径必须能推导 bundled extension 目录");
    assert_eq!(
        extension,
        Path::new("/Applications/Norma OS.app/Contents/Resources/browser-auth-extension"),
        "Chrome 安装指引必须指向 app Resources 中随包扩展目录"
    );
    assert!(
        !extension.starts_with("/Applications/Norma OS.app/Contents/MacOS"),
        "扩展目录不得错误指向可执行 helper 所在的 MacOS 目录"
    );
}

/// CAH-4：Native Messaging 读写双方都必须在分配前拒绝超过 1 MiB 的 frame。
///
/// 数据构造（含关键数值的推导过程）：
///   max = 1024 * 1024 = 1_048_576 bytes；
///   oversized = max + 1 = 1_048_577 bytes；
///   read fixture = 4-byte oversized prefix，不附加 payload，用于证明先验拒绝；
///   write fixture = oversized 个 0 byte。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. reader 读取 oversized prefix，在分配/读取 payload 前返回 InvalidData。
///   2. writer 收到 oversized slice，在写 prefix 前返回 InvalidInput。
///   3. 检查 writer 仍为空，证明没有留下半帧。
///
/// 预期结果：
///   - 正断言：read/write 分别返回 InvalidData 与 InvalidInput。
///   - 负断言：writer 不得写出任何 oversized frame bytes。
#[test]
fn test_native_message_rejects_oversized_frames_before_payload_io() {
    let oversized = MAX_NATIVE_MESSAGE_BYTES + 1;
    let prefix = (oversized as u32).to_ne_bytes();
    let read_error = read_native_message(&mut Cursor::new(prefix))
        .expect_err("超过 1 MiB 的 read frame 必须在 payload 分配前被拒绝");
    assert_eq!(
        read_error.kind(),
        std::io::ErrorKind::InvalidData,
        "oversized read frame 必须归类为 InvalidData"
    );

    let mut output = Vec::new();
    let write_error = write_native_message(&mut output, &vec![0; oversized])
        .expect_err("超过 1 MiB 的 write frame 必须在写 prefix 前被拒绝");
    assert_eq!(
        write_error.kind(),
        std::io::ErrorKind::InvalidInput,
        "oversized write frame 必须归类为 InvalidInput"
    );
    assert!(
        output.is_empty(),
        "拒绝 oversized write 后不得留下可被接收方误读的半帧"
    );
}

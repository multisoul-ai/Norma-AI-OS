use crate::cards::browser::normalize_browser_url;

/// Browser URL 规范化：裸域名补 https，已有 http/https scheme 保持不变。
///
/// 数据构造（含关键数值的推导过程）：
///   bare = "example.com/docs"，scheme 数量 = 0，应补 "https://"
///   https = "https://example.com/docs"，已有 scheme，应保持不变
///   http = "http://localhost:3000"，已有 scheme，应保持不变
///
/// 执行过程（逐步说明系统如何处理）：
///   1. normalize_browser_url(bare)
///   2. normalize_browser_url(https)
///   3. normalize_browser_url(http)
///
/// 预期结果：
///   - 正断言：裸域名变成 https://example.com/docs
///   - 正断言：http/https 输入保持原始 scheme
///   - 负断言：https 输入不应被重复补成 https://https://...
#[test]
fn test_normalize_browser_url_adds_https_only_for_bare_hosts() {
    let bare = normalize_browser_url("example.com/docs");
    let https = normalize_browser_url("https://example.com/docs");
    let http = normalize_browser_url("http://localhost:3000");

    assert_eq!(
        bare, "https://example.com/docs",
        "裸域名应自动补 https://，方便用户直接输入网站地址"
    );
    assert_eq!(
        https, "https://example.com/docs",
        "已有 https:// scheme 时不应修改 URL"
    );
    assert_eq!(
        http, "http://localhost:3000",
        "已有 http:// scheme 时应保留，支持本地开发服务"
    );
    assert_ne!(
        https, "https://https://example.com/docs",
        "已有 scheme 的 URL 不应被重复补 https://"
    );
}

/// Browser URL 规范化：清理空白，空输入回退默认首页。
///
/// 数据构造（含关键数值的推导过程）：
///   spaced = "  docs.example.com  "，前后空白各 2 个，应 trim
///   empty = "   "，trim 后长度 = 0，应回退默认 URL
///   default = "https://www.google.com"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. normalize_browser_url(spaced)
///   2. normalize_browser_url(empty)
///
/// 预期结果：
///   - 正断言：前后空白被移除，并补 https://
///   - 正断言：空输入回退 https://www.google.com
///   - 负断言：结果不应保留首尾空白
#[test]
fn test_normalize_browser_url_trims_and_defaults_empty_input() {
    let spaced = normalize_browser_url("  docs.example.com  ");
    let empty = normalize_browser_url("   ");

    assert_eq!(
        spaced, "https://docs.example.com",
        "URL 输入应先 trim，再为裸 host 补 https://"
    );
    assert_eq!(
        empty, "https://www.google.com",
        "空输入应回退默认浏览器首页"
    );
    assert!(
        !spaced.starts_with(' ') && !spaced.ends_with(' '),
        "规范化后的 URL 不应保留首尾空白"
    );
    assert_ne!(
        empty, "",
        "空输入不应生成空 URL，否则 WebView load_url 会失败"
    );
}

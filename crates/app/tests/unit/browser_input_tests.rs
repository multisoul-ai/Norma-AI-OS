use super::browser_input;

/// 场景：同一地址栏接受开发服务器、普通网址和搜索词。
/// 数据构造：无协议 localhost、显式 HTTPS、带空格的搜索词及危险/本地 URI。
/// 执行过程：通过纯输入适配器解析。
/// 预期结果：loopback 默认 HTTP；普通站点默认 HTTPS；搜索正确编码；不把本地文件路径当网络请求。
#[test]
fn address_input_handles_localhost_search_and_rejected_schemes() {
    assert_eq!(
        browser_input("localhost:3000").expect("本地"),
        "http://localhost:3000/",
        "开发服务器默认 HTTP"
    );
    assert_eq!(
        browser_input("localhost").expect("本地"),
        "http://localhost/",
        "无端口 localhost 也不是搜索词"
    );
    assert_eq!(
        browser_input("example.com").expect("网址"),
        "https://example.com/",
        "外部站点默认 HTTPS"
    );
    assert_eq!(
        browser_input("https://localhost:3000").expect("显式协议"),
        "https://localhost:3000/",
        "尊重显式 HTTPS"
    );
    let search =
        url::Url::parse(&browser_input("rust borrow checker").expect("搜索")).expect("URL");
    assert_eq!(
        search
            .query_pairs()
            .find(|(k, _)| k == "q")
            .expect("搜索参数")
            .1,
        "rust borrow checker",
        "搜索词正确编码"
    );
    assert!(
        browser_input("javascript:alert(1)").is_err(),
        "地址栏不运行脚本 URI"
    );
    assert!(
        browser_input("/Users/person/private notes.md").is_err(),
        "不能把本地路径发送到搜索服务"
    );
}

/// 场景：不支持的 URI 中包含空格，也不能被发送到搜索服务。
/// 数据构造：javascript/data/file 的空白与大小写变体，以及合法开发地址、普通搜索。
/// 执行过程：直接调用地址栏纯解析函数。
/// 预期结果：三种受拒协议返回错误，正常 URL 和搜索仍可解析。
#[test]
fn unsupported_schemes_with_spaces_are_not_sent_to_search() {
    for input in [
        "javascript: alert(1)",
        " JavaScript: alert(1) ",
        "data:text/plain,private token",
        "DATA: private token",
        "file: /Users/person/private notes.md",
        "FILE: private notes.md",
    ] {
        assert!(
            browser_input(input).is_err(),
            "不支持的协议不能作为搜索词发送：{input}"
        );
    }
    assert!(browser_input("localhost:3000").is_ok(), "开发地址仍可打开");
    assert!(
        browser_input("rust borrow checker").is_ok(),
        "普通搜索不受影响"
    );
}

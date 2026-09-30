use super::local_audience;

/// 本地联调只接受 TCP 端口，不能把凭据发送到外部地址或在非法配置时回退生产。
///
/// 数据构造：58477 为普通本地端口；0 无可用目标；65536 = u16::MAX + 1 超出端口范围。
/// 执行过程：构造回环 audience，再分别输入零、溢出、URL 与空配置。
/// 预期结果：合法值生成精确回环地址；非法值均报错，不能接受任意 URL。
#[test]
fn local_relay_port_never_becomes_an_external_origin() {
    assert_eq!(
        local_audience("58477").expect("有效本地端口"),
        "http://127.0.0.1:58477",
        "本地联调必须固定回环地址"
    );
    assert!(local_audience("0").is_err(), "零端口必须拒绝");
    assert!(local_audience("65536").is_err(), "越界端口必须拒绝");
    assert!(
        local_audience("https://example.com").is_err(),
        "端口配置不能扩展为任意外部地址"
    );
    assert!(local_audience("").is_err(), "空配置不能静默回退生产");
}

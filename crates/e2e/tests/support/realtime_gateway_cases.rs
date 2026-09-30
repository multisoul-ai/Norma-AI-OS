use super::support::{wire, *};
use serde_json::json;
use std::sync::atomic::Ordering;

/// 豆包媒体使用独立短期票据加入Mac工具通道；不能伪装GPT身份或绕过项目权限。
///
/// 数据构造：一个隔离TLS Mac、control手机、doubao_A媒体；票据到期=当前秒+30。
///   网关挂载/norma/voice，Mac必须保留路径且拒绝错误API尾缀或编码斜杠。
/// 执行过程：拒绝过期/错协议/错路径/错身份 → 两次绑定同媒体并重放同function。
/// 预期结果：合法豆包原话只执行一次，回执完全一致；不要求OpenAI长期凭据。
#[test]
fn doubao_binding_uses_scoped_ticket_and_durable_receipt() {
    let fixture = Fixture::new(|| {}, accepted);
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
        + 30;
    let url = format!(
        "ws://{}/norma/voice/v1/tools",
        fixture.upstream.local_addr().expect("address")
    );
    let request = |call: &str, generation, url: &str, expiry| {
        json!({
        "type":"bind_realtime","device_id":"phone-a","token":"a".repeat(64),
        "binding":{"call_id":call,"workspace_id":fixture.workspace,"generation":generation},
        "gateway":{"url":url,"token":"b".repeat(43),"expires_at":expiry}})
    };
    let bind = |body| {
        let mut phone = wire::connect(fixture.server.addresses()[0], fixture.cert.clone());
        let result = wire::exchange(&mut phone, &fixture.machine, "bind", body);
        (phone, result)
    };
    assert_eq!(
        bind(request("doubao_A", 1, &url, 0)).1["error"],
        "invalid_request",
        "过期票据不能连接上游"
    );
    assert_eq!(
        bind(request("rtc_A", 1, &url, expires)).1["error"],
        "invalid_request",
        "豆包票据不能绑定GPT身份"
    );
    assert_eq!(
        bind(request("doubao_A", 1, "ws://example.com/v1/tools", expires)).1["error"],
        "invalid_request",
        "明文公网不能接收票据"
    );
    assert_eq!(
        bind(request(
            "doubao_A",
            1,
            &(url.clone() + "?token=bad"),
            expires
        ))
        .1["error"],
        "invalid_request",
        "不能把凭据放入任意查询地址"
    );
    let mut original = None;
    assert_eq!(
        bind(request(
            "doubao_A",
            1,
            &url.replace("/v1/tools", "/v1/voice"),
            expires
        ))
        .1["error"],
        "invalid_request",
        "媒体路径不能冒充工具票据"
    );
    assert_eq!(
        bind(request(
            "doubao_A",
            1,
            &url.replace("/norma/voice", "/norma%2Fvoice"),
            expires
        ))
        .1["error"],
        "invalid_request",
        "编码斜杠不能由代理二次解码为另一个服务"
    );
    for generation in [1, 2] {
        let (phone, result) = bind(request("doubao_A", generation, &url, expires));
        assert_eq!(
            result["result"]["state"], "connecting",
            "合法豆包票据应开始绑定：{result}"
        );
        assert!(
            result["result"].get("gateway").is_none(),
            "绑定回执不能回显秘密票据"
        );
        let mut peer = attach(&fixture.upstream);
        turn(&mut peer, "doubao-turn", "请让 Codex 实现登录校验");
        call(
            &mut peer,
            "doubao-response",
            "doubao-function",
            "实现登录校验",
        );
        let receipt = output(&mut peer, "doubao-function");
        assert_eq!(receipt["status"], "accepted", "应复用真实Mac接收回执");
        if let Some(expected) = &original {
            assert_eq!(&receipt, expected, "重绑只能读原回执");
        } else {
            original = Some(receipt);
        }
        continuation(&mut peer);
        drop(phone);
        drop(peer);
    }
    assert_eq!(
        fixture.writes.load(Ordering::SeqCst),
        1,
        "豆包重连不应产生第二次写操作"
    );
}

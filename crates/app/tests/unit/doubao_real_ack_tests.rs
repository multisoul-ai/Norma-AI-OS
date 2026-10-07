use super::{validate_asr_ack, Message};

fn response(kind: u8, code: u32) -> Message {
    let payload = if kind == 15 {
        br#"{"message":"synthetic rejection"}"#.to_vec()
    } else {
        serde_json::json!({"code":code}).to_string().into_bytes()
    };
    // V3 header = 4 B, JSON serialization = 1, compression = 0, no sequence flag.
    let mut frame = vec![0x11, kind << 4, 0x10, 0];
    if kind == 15 {
        frame.extend_from_slice(&code.to_be_bytes());
    }
    frame.extend_from_slice(
        &u32::try_from(payload.len())
            .expect("测试 payload 应小于 4 GiB")
            .to_be_bytes(),
    );
    frame.extend_from_slice(&payload);
    Message::Binary(frame.into())
}

/// 真实 ASR smoke 的验收不能把 Binary 服务拒绝帧误判为成功。
///
/// 数据构造（含关键数值的推导过程）：成功帧使用 0x9 与 code=20,000,000；
///   错误帧使用 0xF 与复现中的 code=45,000,081，JSON 内没有 code；
///   错误帧长度 = 4 B 基础头 + 4 B 错误码 + 4 B 长度 + JSON。
/// 执行过程：
///   1. 向真实服务测试所用的验收函数提交合法成功帧。
///   2. 提交同为 Binary 的拒绝帧，提取错误诊断。
///
/// 预期结果：成功帧通过；拒绝帧失败且包含 45000081，不能以缺失 JSON code 当作成功。
#[test]
fn rejects_binary_asr_error_ack() {
    assert!(
        validate_asr_ack(&response(9, 20_000_000)).is_ok(),
        "合法 ASR 确认必须通过"
    );
    let rejected = response(15, 45_000_081);
    let error = validate_asr_ack(&rejected).expect_err("Binary 服务拒绝帧不得通过真实测试验收");
    assert!(
        error
            .resolve_for(ui::i18n::Locale::ZhCn)
            .contains("45000081"),
        "诊断必须保留拒绝帧的真实错误码"
    );
    assert!(
        validate_asr_ack(&rejected).is_err(),
        "错误帧不得伪装成配置接受"
    );
}

/// 普通 ASR 响应中的 JSON 拒绝码也不能通过真实配置验收。
///
/// 数据构造（含关键数值的推导过程）：两个 0x9 响应分别携带 JSON code=0（成功）
///   和 code=45,000,081（拒绝）；普通帧长度 = 4 B 基础头 + 4 B 长度 + JSON。
/// 执行过程：
///   1. 提交 code=0 的正常确认。
///   2. 提交帧类型正常、但 JSON code 非成功的确认。
///
/// 预期结果：合法确认通过；JSON 拒绝失败，诊断保留 45000081，不能被当作成功码 0。
#[test]
fn rejects_json_asr_error_ack() {
    assert!(
        validate_asr_ack(&response(9, 0)).is_ok(),
        "JSON 成功码 0 应通过"
    );
    let error =
        validate_asr_ack(&response(9, 45_000_081)).expect_err("正常帧类型不能掩盖 JSON 拒绝码");
    let diagnostic = error.resolve_for(ui::i18n::Locale::ZhCn);
    assert!(
        diagnostic.contains("45000081"),
        "应报告 JSON 中的真实拒绝码"
    );
    assert!(
        !diagnostic.contains("错误码 0"),
        "不得把 JSON 拒绝码变成成功码 0"
    );
}

/// 非法二进制帧及旧 JSON 控制消息均不能通过 ASR 配置验收。
///
/// 数据构造（含关键数值的推导过程）：合法 0x9 确认作为正例；截断帧只有基础头 4 B，
///   小于 decoder 所需最短 12 B；Text 载荷即使含成功码也属于错误消息类型。
/// 执行过程：
///   1. 提交合法 Binary 确认，再提交只有基础头的 Binary 截断帧。
///   2. 提交带成功码的 Text 消息。
///
/// 预期结果：合法确认通过；截断 Binary 与 Text 均失败，不会把消息可读当作服务接受。
#[test]
fn rejects_malformed_and_nonbinary_asr_ack() {
    assert!(
        validate_asr_ack(&response(9, 20_000_000)).is_ok(),
        "合法确认应通过"
    );
    let malformed = Message::Binary(vec![0x11, 0x90, 0x10, 0].into());
    assert!(
        validate_asr_ack(&malformed).is_err(),
        "缺失长度及载荷的 Binary 不得通过"
    );
    let text = Message::Text(r#"{"code":20000000}"#.into());
    assert!(
        validate_asr_ack(&text).is_err(),
        "Text 成功码不能伪装成 ASR 二进制确认"
    );
}

/// 错误帧的纯文本载荷不能覆盖协议头中的真实拒绝码。
///
/// 数据构造（含关键数值的推导过程）：0xF 错误帧使用 45,000,081，载荷为纯文本诊断；
///   分别验证序列化标志 1（JSON）和 0（无序列化），错误码独立存储在协议头。
///   固定部分 = 4 B 基础头 + 4 B 错误码 + 4 B 长度 = 12 B。
/// 执行过程：
///   1. 将 JSON 错误帧的载荷替换为纯文本，并更新长度。
///   2. 经真实 smoke 使用的生产 decoder 校验。
///
/// 预期结果：仍然拒绝配置并保留 45000081；不能因为载荷不是 JSON 而丢失拒绝码。
#[test]
fn preserves_asr_error_code_with_plaintext_payload() {
    let Message::Binary(frame) = response(15, 45_000_081) else {
        panic!("测试构造器应返回 Binary");
    };
    let mut frame = frame.to_vec();
    let payload = b"quota exceeded";
    frame.truncate(12);
    frame[8..12].copy_from_slice(
        &u32::try_from(payload.len())
            .expect("短诊断应可编码长度")
            .to_be_bytes(),
    );
    frame.extend_from_slice(payload);
    let error = validate_asr_ack(&Message::Binary(frame.clone().into()))
        .expect_err("纯文本错误帧也必须拒绝配置");
    let diagnostic = error.resolve_for(ui::i18n::Locale::ZhCn);
    assert!(
        diagnostic.contains("45000081"),
        "必须保留协议头错误码而非只报 JSON 解析失败"
    );
    assert!(
        !diagnostic.contains("JSON"),
        "错误帧载荷无需作为识别 JSON 解析"
    );
    // Serialization 0 + compression 0 = header byte 0x00.
    frame[2] = 0;
    let raw_error =
        validate_asr_ack(&Message::Binary(frame.into())).expect_err("无序列化错误帧也必须拒绝配置");
    assert!(
        raw_error
            .resolve_for(ui::i18n::Locale::ZhCn)
            .contains("45000081"),
        "无序列化错误帧也应保留真实拒绝码"
    );
}

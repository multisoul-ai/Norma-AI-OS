use norma::mobile::{ConversationFragment, CONVERSATION_CHUNK_BYTES};

/// 超过单帧上限的Unicode正文按UTF-8边界分段，重组不能缺字或把转义误当字节数。
///
/// 数据构造：3字节中文重复2×64KiB次，正文384KiB，加引号/换行测试JSON转义。
/// 执行过程：逐段读取原始UTF-8 → JSON往返 → 按offset重组 → 非字符边界/EOF请求拒绝。
/// 预期结果：每片原始字节≤64KiB、wire<500KiB；正文完全相同，非法位置不能返回空成功片。
#[test]
fn conversation_fragments_preserve_unicode_and_bounded_json() {
    let body = format!("{}\"\n", "长".repeat(2 * CONVERSATION_CHUNK_BYTES));
    let mut rebuilt = String::new();
    while rebuilt.len() < body.len() {
        let fragment =
            ConversationFragment::at("transfer-a", &body, rebuilt.len()).expect("有效位置");
        assert_eq!(
            fragment.offset,
            rebuilt.len(),
            "offset必须是原始UTF-8字节数"
        );
        assert_eq!(fragment.total_bytes, body.len(), "各片冻结相同总长度");
        assert!(
            fragment.text.len() <= CONVERSATION_CHUNK_BYTES,
            "单片不能超64KiB"
        );
        let wire = serde_json::to_vec(&fragment).expect("编码");
        assert!(wire.len() < 500 * 1024, "含JSON转义也必须低于手机单帧上限");
        let decoded: ConversationFragment = serde_json::from_slice(&wire).expect("传输解码");
        rebuilt.push_str(&decoded.text);
    }
    assert_eq!(rebuilt, body, "长正文不得丢字、截断或重复");
    assert!(
        ConversationFragment::at("transfer-a", &body, 1).is_err(),
        "中文中间字节不能做分页起点"
    );
    assert!(
        ConversationFragment::at("transfer-a", &body, body.len()).is_err(),
        "EOF不能生成空片陷入无限读取"
    );
}

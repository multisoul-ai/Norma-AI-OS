use std::collections::HashSet;

/// 场景：每个控制连接需要真实 OS 随机身份，不能依赖 Unix 特有设备或时间回退。
/// 构造：64 个 UUID v4；保留 122 个随机位，样本碰撞上界约为 2016 / 2^122。
/// 执行：调用公开生产工厂，逐个解析 UUID 字节、检查版本/变体并加入集合。
/// 预期：所有调用成功、格式为 8-4-4-4-12、版本与变体正确，64 个身份无重复。
#[test]
fn control_identity_uses_os_entropy_and_preserves_uuid_v4_contract() {
    let mut ids = HashSet::new();
    for _ in 0..64 {
        let id = norma::agent_hook::fresh_control_id().expect("每个平台都必须取得 OS 安全随机身份");
        assert_eq!(
            id.len(),
            36,
            "UUID 字符长度保持 32 个十六进制字符与 4 个分隔符"
        );
        assert_eq!(
            id.split('-').map(str::len).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12],
            "控制身份不能改成时间或路径"
        );
        let bytes = hex::decode(id.replace('-', "")).expect("身份必须为十六进制 UUID");
        assert_eq!(bytes[6] >> 4, 4, "必须保留 UUID v4 版本");
        assert_eq!(bytes[8] >> 6, 2, "必须保留 RFC 4122 变体 10");
        assert!(ids.insert(id), "独立控制连接不能复用身份");
    }
    assert_eq!(ids.len(), 64, "所有连接都拥有独立身份");
}

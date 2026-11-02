#[path = "../../app/src/norma/route_id.rs"]
mod route_id;
use std::collections::HashSet;

/// 场景：App工作区、手机会话、流订阅必须使用原生随机ID，不能依赖Unix设备文件。
/// 构造：真实生产入口连续生成1000个128bit UUID；只固定RFC版本/variant位。
/// 执行：验证格式、OS随机源可用和本批唯一性；预期Windows与macOS均成功。
/// 1000次覆盖多次原生调用，不把批内唯一性当作完整熵质量证明。
#[test]
fn app_routes_use_available_native_randomness_and_rfc4122_v4_ids() {
    let mut seen = HashSet::new();
    for _ in 0..1_000 {
        let id = route_id::new_workspace_route_id().expect("App路由ID必须使用当前OS可用的随机源");
        assert_eq!(
            id.split('-').map(str::len).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12],
            "路由ID必须保留标准UUID文本布局"
        );
        let bytes = hex::decode(id.replace('-', "")).expect("UUID必须使用合法十六进制");
        assert_eq!(bytes[6] >> 4, 4, "路由ID必须保留RFC4122 v4版本位");
        assert_eq!(bytes[8] >> 6, 2, "路由ID必须保留RFC4122 variant位10");
        assert!(seen.insert(id), "重复ID会混淆工作区或手机流生命周期");
    }
    assert_eq!(seen.len(), 1_000, "所有原生调用均应成功生成独立路由ID");
}

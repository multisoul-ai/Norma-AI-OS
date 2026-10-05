use super::*;

/// 桌面任务的交付源：持久任务引用唯一决定预留操作，运行 ID 与另一项目不能混用。
/// 数据构造：同任务 task-a 在区域 a/b；task-b 是新任务；route-a 是已保留会话路由。
/// 执行过程：生成三份真实任务源 → 比较操作身份 → 在普通旧项目尝试生成。
/// 预期结果：原区域可生成；换区域或任务有新身份；旧项目不新增交付自动捕获。
#[test]
fn desktop_delivery_source_binds_area_and_durable_task() {
    let first =
        desktop_delivery_source("desktop:space:area-a", "route-a", "task-a").expect("桌面任务源");
    let same =
        desktop_delivery_source("desktop:space:area-a", "route-a", "task-a").expect("相同源");
    let other =
        desktop_delivery_source("desktop:space:area-b", "route-a", "task-a").expect("其他区域");
    let later =
        desktop_delivery_source("desktop:space:area-a", "route-a", "task-b").expect("新任务");
    assert_eq!(
        first.operation_id, same.operation_id,
        "同任务捕获不能产生两个预留"
    );
    assert_ne!(
        first.operation_id, other.operation_id,
        "跨区域不能冒领原始起点"
    );
    assert_ne!(
        first.operation_id, later.operation_id,
        "新任务不能复用旧起点"
    );
    assert!(
        desktop_delivery_source("project-a", "route-a", "task-a").is_none(),
        "旧手机路径沿用既有交付捕获，不新增另一份"
    );
}

use super::{phase_for_integrations, SharedHealthPhase, SharedOrchestrationHealth};
use norma::orchestration::{ComponentState, OrchestrationProvider, ProviderIntegration};

fn ready_integration(provider: OrchestrationProvider) -> ProviderIntegration {
    let ready = ComponentState::Ready {
        version: "2.0.0".to_string(),
    };
    ProviderIntegration::from_components(
        provider,
        ready.clone(),
        ready.clone(),
        ready.clone(),
        ready,
    )
}

/// 全局健康检查代次：多个窗口注册只能启动一次扫描，并共享同一最终阶段。
///
/// 数据构造（含关键数值的推导过程）：
///   window registrations = first(1) + second(1) = 2 次
///   refresh starts = 1 次（2 次注册折叠为 1 次全局扫描）
///   providers = Ready Codex(1) + Missing Claude(1) = 2 个代表状态
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 checking 状态并连续请求两次 begin_refresh。
///   2. 用全部 Ready 集成计算 ready 阶段。
///   3. 把一个 Provider 的 Hook 改为 Missing，计算 degraded 阶段。
///
/// 预期结果：
///   - 正断言：首次注册取得扫描权，第二次不会重复启动扫描。
///   - 正断言：全部组件 Ready 时为 ready，任一受支持 Provider 缺组件时为 degraded。
///   - 负断言：checking 不得被当作 degraded/untracked，也不得产生第二次扫描。
#[test]
fn global_health_refresh_is_single_flight_and_classifies_one_shared_snapshot() {
    let mut health = SharedOrchestrationHealth::default();

    let first_generation = health.begin_refresh();
    let second_generation = health.begin_refresh();
    let ready_phase = phase_for_integrations(&[
        ready_integration(OrchestrationProvider::Codex),
        ready_integration(OrchestrationProvider::Claude),
    ]);
    let degraded_phase = phase_for_integrations(&[
        ready_integration(OrchestrationProvider::Codex),
        ProviderIntegration::from_components(
            OrchestrationProvider::Claude,
            ComponentState::Ready {
                version: "2.0.0".to_string(),
            },
            ComponentState::Ready {
                version: "2.0.0".to_string(),
            },
            ComponentState::Missing,
            ComponentState::Ready {
                version: "2.0.0".to_string(),
            },
        ),
    ]);

    assert!(
        first_generation.is_some(),
        "第一个窗口注册必须取得全局健康扫描的启动权"
    );
    assert!(
        second_generation.is_none(),
        "第二个窗口注册不得并发启动第二份 Provider 扫描"
    );
    assert_eq!(
        health.phase(),
        SharedHealthPhase::Checking,
        "扫描完成前必须保持 checking，不能预先投影为 untracked"
    );
    assert_eq!(
        ready_phase,
        SharedHealthPhase::Ready,
        "所有受检 Provider 组件 Ready 时全局阶段必须为 ready"
    );
    assert_eq!(
        degraded_phase,
        SharedHealthPhase::Degraded,
        "任一受支持 Provider 缺少 Hook 时全局阶段必须为 degraded"
    );
    assert_ne!(
        health.phase(),
        SharedHealthPhase::Degraded,
        "尚在检查不能伪装成 degraded，否则 Agent 会被提前标记 untracked"
    );
}

/// 全局健康快照代次：较新的发布结果必须阻止旧后台扫描回写覆盖。
///
/// 数据构造（含关键数值的推导过程）：
///   background refresh generations = stale(1)
///   explicit publish generations = current(1)
///   Total generations = 1 + 1 = 2，current 必须严格大于 stale
///   current providers = Ready Codex(1)
///   stale providers = Missing Codex(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 begin_refresh，模拟 CTA 触发的后台扫描并记录 stale generation。
///   2. 在扫描结束前调用 begin_publish，模拟安装完成后的较新共享快照。
///   3. 先提交 current Ready 快照，再让 stale Missing 扫描迟到完成。
///   4. 读取全局阶段与保存的 Codex 状态。
///
/// 预期结果：
///   - 正断言：current 发布被接受并保持 Ready。
///   - 正断言：stale 完成返回 None，不通知订阅窗口。
///   - 负断言：迟到扫描不得把最新 Ready 快照降级为 Missing/Degraded。
#[test]
fn newer_health_publish_rejects_stale_background_refresh_completion() {
    let mut health = SharedOrchestrationHealth::default();
    let stale_generation = health
        .begin_refresh()
        .expect("首次后台扫描必须取得 stale generation");
    let current_generation = health.begin_publish();

    assert!(
        current_generation > stale_generation,
        "显式发布的 generation 必须严格晚于仍在运行的后台扫描"
    );

    let current_snapshot = health.finish(
        current_generation,
        Ok((
            vec![ready_integration(OrchestrationProvider::Codex)],
            Vec::new(),
        )),
    );
    let stale_snapshot = health.finish(
        stale_generation,
        Ok((
            vec![ProviderIntegration::from_components(
                OrchestrationProvider::Codex,
                ComponentState::Missing,
                ComponentState::Missing,
                ComponentState::Missing,
                ComponentState::Missing,
            )],
            Vec::new(),
        )),
    );

    assert!(
        current_snapshot.is_some(),
        "较新的 Ready 发布必须被健康状态接受"
    );
    assert!(
        stale_snapshot.is_none(),
        "迟到的旧扫描必须被 generation guard 丢弃"
    );
    assert_eq!(
        health.phase(),
        SharedHealthPhase::Ready,
        "旧 Missing 扫描不得把最新 Ready 阶段覆盖为 Degraded"
    );
    assert_eq!(
        health
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.integrations.first())
            .map(|integration| integration.aggregate_status),
        Some(norma::orchestration::AggregateStatus::Ready),
        "健康状态必须保留较新发布的 Codex Ready 快照"
    );
}

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

    let first_started = health.begin_refresh();
    let second_started = health.begin_refresh();
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

    assert!(first_started, "第一个窗口注册必须取得全局健康扫描的启动权");
    assert!(
        !second_started,
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

use super::*;

/// DR-M-7：每个标准命中必须复用固定 28 粒子池，并保持主彗星、离子团与延迟火花的明确组成。
///
/// 数据构造（含关键数值的推导过程）：
///   primary particles = 20
///   primary motes = 每第 5 颗 1 个 = 20 / 5 = 4
///   primary comets = 20 - 4 = 16
///   secondary sparks = 8
///   total = 16 + 4 + 8 = 28
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建一条非 Reduce Motion 的 Submitted dispatch receipt。
///   2. 从固定数组读取实际启用的 28 颗粒子。
///   3. 按 kind 统计主彗星、离子团和延迟火花，并检查火花延迟/寿命边界。
///
/// 预期结果：
///   - 正断言：组成精确为 16 Comet、4 Mote、8 Spark。
///   - 正断言：Spark 均延迟到命中后出现，所有粒子寿命均为正数。
///   - 负断言：池不得退回旧 18 粒子版本，也不得出现未分类粒子。
#[test]
fn test_dispatch_receipt_builds_fixed_layered_ion_particle_pool() {
    let mut store = MotionStore::new(false);
    store.spawn_dispatch_receipt("runtime-ion:1", 971, DispatchReceiptVisualStatus::Submitted);
    let presentation = store.dispatch_receipt_presentations()[0];
    let active = &presentation.particles[..presentation.particle_count];
    let comet_count = active
        .iter()
        .filter(|particle| particle.kind == DispatchImpactParticleKind::Comet)
        .count();
    let mote_count = active
        .iter()
        .filter(|particle| particle.kind == DispatchImpactParticleKind::Mote)
        .count();
    let spark_count = active
        .iter()
        .filter(|particle| particle.kind == DispatchImpactParticleKind::Spark)
        .count();
    let delayed_sparks = active
        .iter()
        .filter(|particle| {
            particle.kind == DispatchImpactParticleKind::Spark && particle.delay >= 0.12
        })
        .count();
    let positive_lifetimes = active
        .iter()
        .filter(|particle| particle.lifetime > 0.0)
        .count();

    assert_eq!(
        presentation.particle_count, 28,
        "标准离子命中必须启用 20+8=28 颗固定池粒子"
    );
    assert_ne!(
        presentation.particle_count, 18,
        "离子命中不得回退为旧版 18 颗单层圆点"
    );
    assert_eq!(comet_count, 16, "20 颗主粒子中必须有精确 16 颗 Comet");
    assert_eq!(mote_count, 4, "20 颗主粒子中每第 5 颗为 Mote，共 4 颗");
    assert_eq!(spark_count, 8, "次级爆发必须包含精确 8 颗 Spark");
    assert_eq!(
        delayed_sparks, 8,
        "8 颗 Spark 都必须在归一化进度 0.12 后延迟出现"
    );
    assert_eq!(
        positive_lifetimes, 28,
        "28 颗活动粒子都必须拥有正寿命，不能出现未初始化槽位"
    );
    assert_eq!(
        comet_count + mote_count + spark_count,
        28,
        "所有活动粒子必须被唯一分类为 Comet、Mote 或 Spark"
    );
}

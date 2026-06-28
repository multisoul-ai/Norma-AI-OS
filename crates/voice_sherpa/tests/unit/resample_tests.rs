use super::LinearResampler;

const EPS: f32 = 1e-4;

/// RESAMPLE-1：整数倍降采样（48000 → 16000，step = 3.0）保持常量信号原值。
///
/// 数据构造（含关键数值的推导过程）：
///   src_rate = 48000 Hz，dst_rate = 16000 Hz
///   step     = src/dst = 48000 / 16000 = 3.0（每个输出样本前进 3 个输入样本）
///   input    = [0.5; 300]（300 个常量样本 ≈ 6.25ms @48k）
///   预期输出长度 ≈ 300 / 3 = 100（循环条件 pos+1 < 300 → pos ∈ {0,3,…,297}，共 100 个）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. tail 为空，pos = 0，buf = input（len = 300）
///   2. 在 pos = 0,3,…,297 处线性插值；常量信号插值结果恒为 0.5
///   3. pos 越过 298 后停止，剩余样本进入 tail 供下一块使用
///
/// 预期结果：
///   - 正断言：输出长度落在 [99,101]（≈100，允许 ±1 的相位边界误差）
///   - 正断言：每个输出样本都等于 0.5（常量信号经线性插值不变）
///   - 负断言：不应出现 0.0 静音样本，否则说明信号被丢弃成了静音
#[test]
fn test_downsample_constant_signal_preserves_value() {
    let mut resampler = LinearResampler::new(48_000.0, 16_000.0);
    let input = vec![0.5f32; 300];

    let out = resampler.process(&input);

    assert!(
        (99..=101).contains(&out.len()),
        "48k→16k 降采样 300 样本应产出 ≈100 个样本，实际 {}",
        out.len()
    );
    assert!(
        out.iter().all(|&s| (s - 0.5).abs() < EPS),
        "常量 0.5 信号经线性插值后每个样本仍应为 0.5"
    );
    assert!(
        !out.iter().any(|&s| s.abs() < EPS),
        "输出不应出现 0.0 静音样本，否则说明输入信号被丢弃"
    );
}

/// RESAMPLE-2：同采样率（16000 → 16000，step = 1.0）近似恒等映射。
///
/// 数据构造（含关键数值的推导过程）：
///   step  = 16000 / 16000 = 1.0（输出样本与输入样本一一对应）
///   input = [0,1,2,3,4]（递增斜坡，便于逐点核对）
///   预期：pos = 0,1,2,3（pos+1 < 5），输出 [0,1,2,3]，最后一个样本留作 tail
///
/// 执行过程（逐步说明系统如何处理）：
///   1. tail 为空，pos = 0，buf = [0,1,2,3,4]
///   2. pos = 0→0.0, 1→1.0, 2→2.0, 3→3.0，frac 恒为 0，直接取整数位样本
///   3. pos = 4 时 4+1 < 5 不成立，停止；buf[4] 保留进 tail
///
/// 预期结果：
///   - 正断言：输出长度为 4（5 个输入样本里末个留作跨块插值的 tail）
///   - 正断言：output[0]==0.0 且 output[3]==3.0（与输入逐点对应）
///   - 负断言：output[3] 不应等于 4.0（最后一个输入样本此刻还不能被输出）
#[test]
fn test_identity_rate_maps_one_to_one() {
    let mut resampler = LinearResampler::new(16_000.0, 16_000.0);
    let input = vec![0.0f32, 1.0, 2.0, 3.0, 4.0];

    let out = resampler.process(&input);

    assert_eq!(
        out.len(),
        4,
        "step=1 时 5 个输入样本应输出 4 个，末样本留作 tail"
    );
    assert!((out[0] - 0.0).abs() < EPS, "首样本应映射为输入首值 0.0");
    assert!((out[3] - 3.0).abs() < EPS, "第 4 个输出应映射为输入值 3.0");
    assert!(
        (out[3] - 4.0).abs() > EPS,
        "末个输入样本 4.0 此刻应留作 tail，不应被提前输出"
    );
}

/// RESAMPLE-3：跨缓冲相位连续性 —— 分两块送入与一次性送入产生相同输出。
///
/// 数据构造（含关键数值的推导过程）：
///   src_rate = 44100 Hz，dst_rate = 16000 Hz（非整数倍，step ≈ 2.75625，强制走分数相位累加）
///   signal   = 斜坡 [0,1,2,…,599]（600 样本），分割点在 index 300
///   one-shot：单次 process(signal)
///   split   ：process(signal[0..300]) ++ process(signal[300..600])
///   关键不变量：tail 保留 buf[floor(pos)..]，恰好覆盖下一块插值所需的左邻样本，
///              因此“分两次”与“一次性”在数学上等价。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. one-shot 解析器处理整段，累计分数相位
///   2. split 解析器先处理前半段，把跨界所需样本存入 tail 与 pos
///   3. split 处理后半段时，buf = tail ++ 后半段，相位从上次余量继续
///
/// 预期结果：
///   - 正断言：split 输出长度与 one-shot 相差不超过 1（边界余量）
///   - 正断言：对齐区间内每个样本差值 < EPS（证明相位连续、无接缝跳变）
///   - 负断言：split 不应只产出前半段长度（若 tail/pos 丢失会导致后半段无输出或错位）
#[test]
fn test_split_input_matches_single_pass() {
    let signal: Vec<f32> = (0..600).map(|i| i as f32).collect();

    let one_shot = LinearResampler::new(44_100.0, 16_000.0).process(&signal);

    let mut split_resampler = LinearResampler::new(44_100.0, 16_000.0);
    let mut split = split_resampler.process(&signal[0..300]);
    split.extend(split_resampler.process(&signal[300..600]));

    assert!(
        (one_shot.len() as i64 - split.len() as i64).abs() <= 1,
        "分块与单次输出长度应几乎相同：one_shot={}, split={}",
        one_shot.len(),
        split.len()
    );
    let compare = one_shot.len().min(split.len());
    assert!(
        compare > 100,
        "对齐区间应有足够样本（>100）以证明后半段确实被处理，实际 {compare}"
    );
    for i in 0..compare {
        assert!(
            (one_shot[i] - split[i]).abs() < EPS,
            "第 {i} 个样本应连续一致：one_shot={}, split={}（接缝处出现跳变说明相位未延续）",
            one_shot[i],
            split[i]
        );
    }
}

/// RESAMPLE-4：空输入安全返回空，且不破坏后续状态。
///
/// 数据构造（含关键数值的推导过程）：
///   第一次 process(&[])  → 应返回空 Vec，pos / tail 不变
///   随后 process([1.0;48]) → 仍应正常产出 ≈16 个样本（48 / 3）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 空输入触发早返回，不进入插值循环
///   2. 后续非空输入照常处理，证明空调用没有污染内部状态
///
/// 预期结果：
///   - 正断言：空输入输出长度为 0
///   - 正断言：随后非空输入仍能产出样本（>0）
///   - 负断言：不应 panic（数组越界 / 除零）
#[test]
fn test_empty_input_is_safe() {
    let mut resampler = LinearResampler::new(48_000.0, 16_000.0);

    let empty = resampler.process(&[]);
    assert_eq!(empty.len(), 0, "空输入应返回空输出");

    let next = resampler.process(&[1.0f32; 48]);
    assert!(
        !next.is_empty(),
        "空调用后解析器仍应能正常处理非空输入（状态未被破坏）"
    );
}

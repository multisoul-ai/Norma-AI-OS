use super::*;
use audio::{PcmAudioChunk, PcmFormat};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn pcm16(bytes: Vec<u8>) -> PcmAudioChunk {
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, bytes)
        .expect("测试用 16kHz 单声道 PCM16 字节数必须为偶数")
}

/// ACA-1：同名终态片段只能命中所属工作区的缓存，不能跨项目复用。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace A = `/projects/alpha`，缓存 `Raya` + `Done` 两段 PCM
///   workspace B = `/projects/beta`，请求同一 `Raya` + `Done` 组合
///   name PCM    = 4 bytes = 2 个 16-bit 样本
///   outcome PCM = 4 bytes = 2 个 16-bit 样本
///   access gap  = 1s，远小于 30 days，因此不应触发过期清理
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 alpha 写入 `Raya` 和 `Done` 的 PCM 片段。
///   2. 在 alpha 读取同一组合，验证可获得原始名称与结果音频。
///   3. 在 beta 读取相同文本，验证不能命中 alpha 的缓存。
///
/// 预期结果：
///   - 正断言：alpha 命中并返回写入时的名称与结果 PCM。
///   - 负断言：beta 不得得到 alpha 的任一 PCM，避免跨工作区串音。
#[test]
fn test_workspace_cache_does_not_reuse_agent_audio_across_projects() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let alpha = PathBuf::from("/projects/alpha");
    let beta = PathBuf::from("/projects/beta");
    let name_pcm = pcm16(vec![1, 0, 2, 0]);
    let outcome_pcm = pcm16(vec![3, 0, 4, 0]);
    let mut cache = AgentAnnouncementAudioCache::default();

    cache.insert(
        &alpha,
        "Raya",
        "Done",
        name_pcm.clone(),
        outcome_pcm.clone(),
        now,
    );
    let alpha_hit = cache.lookup(&alpha, "Raya", "Done", now + Duration::from_secs(1));
    let beta_hit = cache.lookup(&beta, "Raya", "Done", now + Duration::from_secs(1));

    let alpha_hit = alpha_hit.expect("alpha 的已缓存终态播报必须命中");
    assert_eq!(
        alpha_hit.name.as_ref(),
        &name_pcm,
        "alpha 命中时必须返回属于 alpha 的 Agent 名称 PCM"
    );
    assert_eq!(
        alpha_hit.outcome.as_ref(),
        &outcome_pcm,
        "alpha 命中时必须返回属于 alpha 的 Done PCM"
    );
    assert!(
        beta_hit.is_none(),
        "beta 请求相同文本时不得复用 alpha 的 Agent 终态 PCM"
    );
}

/// ACA-2：用户切换音色后，所有工作区的旧 PCM 必须一起失效。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace count = 2：alpha 与 beta 各缓存 1 组 `Raya` + `Done`
///   segments        = 2 workspaces × 2 texts = 4 个 PCM 片段
///   voice changes   = 1 次显式用户音色切换
///   access gap      = 1s，远小于 30 days，清空原因只能是音色切换
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 alpha、beta 分别写入不同的 `Raya` / `Done` PCM。
///   2. 调用音色变更专用失效入口。
///   3. 在两个工作区读取旧组合，检查旧音频均不可再命中。
///
/// 预期结果：
///   - 负断言：alpha 不得保留切换前音色的 PCM。
///   - 负断言：beta 不得保留切换前音色的 PCM。
///   - 正断言：清空后仍可安全查询，不会因已删除分区而失败。
#[test]
fn test_voice_change_clears_cached_audio_for_every_workspace() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000);
    let alpha = PathBuf::from("/projects/alpha");
    let beta = PathBuf::from("/projects/beta");
    let mut cache = AgentAnnouncementAudioCache::default();
    cache.insert(
        &alpha,
        "Raya",
        "Done",
        pcm16(vec![1, 0]),
        pcm16(vec![2, 0]),
        now,
    );
    cache.insert(
        &beta,
        "Raya",
        "Done",
        pcm16(vec![3, 0]),
        pcm16(vec![4, 0]),
        now,
    );

    cache.clear_for_voice_change();

    let alpha_after_change = cache.lookup(&alpha, "Raya", "Done", now + Duration::from_secs(1));
    let beta_after_change = cache.lookup(&beta, "Raya", "Done", now + Duration::from_secs(1));
    assert!(
        alpha_after_change.is_none(),
        "用户切换音色后 alpha 不得继续命中旧音色 PCM"
    );
    assert!(
        beta_after_change.is_none(),
        "用户切换音色后 beta 不得继续命中旧音色 PCM"
    );
    assert_eq!(
        cache.workspace_count(),
        0,
        "音色切换应删除所有工作区分区，而不是只清空其中一个"
    );
}

/// ACA-3：超过 30 天未访问的工作区分区在下一次缓存访问时自动清除，近期访问的分区保留。
///
/// 数据构造（含关键数值的推导过程）：
///   retention           = 30 days = 30 × 24 × 60 × 60 = 2,592,000s
///   alpha last access   = day 0，距清扫时刻恰好 30 days，达到清理边界
///   beta last access    = day 29，距清扫时刻 1 day，仍在保留窗口内
///   workspace count     = 初始 2，清扫后预期 2 - 1 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 day 0 向 alpha、beta 分别写入终态 PCM。
///   2. 在 day 29 读取 beta，更新它的最近访问时间。
///   3. 在 day 30 再读取 beta，触发惰性清扫并检查两个分区。
///
/// 预期结果：
///   - 正断言：近期访问的 beta 仍命中原始 PCM。
///   - 正断言：清扫后只保留 1 个工作区分区。
///   - 负断言：alpha 达到 30 天边界后不得继续命中旧 PCM。
#[test]
fn test_cache_prunes_workspace_after_thirty_days_without_access() {
    let day = Duration::from_secs(24 * 60 * 60);
    let retention = day * 30;
    let day_zero = SystemTime::UNIX_EPOCH + Duration::from_secs(3_000);
    let alpha = PathBuf::from("/projects/alpha");
    let beta = PathBuf::from("/projects/beta");
    let mut cache = AgentAnnouncementAudioCache::default();
    cache.insert(
        &alpha,
        "Raya",
        "Done",
        pcm16(vec![1, 0]),
        pcm16(vec![2, 0]),
        day_zero,
    );
    cache.insert(
        &beta,
        "Raya",
        "Done",
        pcm16(vec![3, 0]),
        pcm16(vec![4, 0]),
        day_zero,
    );
    let beta_before_boundary = cache.lookup(&beta, "Raya", "Done", day_zero + retention - day);

    let beta_after_cleanup = cache.lookup(&beta, "Raya", "Done", day_zero + retention);
    let alpha_after_cleanup = cache.lookup(&alpha, "Raya", "Done", day_zero + retention);

    assert!(
        beta_before_boundary.is_some(),
        "day 29 访问 beta 时必须仍命中，才能刷新其保留窗口"
    );
    assert_eq!(
        beta_after_cleanup
            .as_ref()
            .map(|segments| segments.name.bytes.as_slice()),
        Some(&[3, 0][..]),
        "day 29 刚访问的 beta 在 day 30 仍必须保留其名称 PCM"
    );
    assert_eq!(
        cache.workspace_count(),
        1,
        "触发惰性清扫后只能保留近期访问的 beta 工作区分区"
    );
    assert!(
        alpha_after_cleanup.is_none(),
        "达到 30 天未访问边界的 alpha 不得继续命中旧终态 PCM"
    );
}

/// ACA-4：名称与结果词文本相同仍必须保存各自 PCM，不能因字符串相同而互相覆盖。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace        = alpha
///   agent name       = `Done`（与结果词完全相同的合法别名）
///   name PCM         = [1, 0]，outcome PCM = [2, 0]（两段内容故意不同）
///   cache keys       = 2 个：名称角色 + `Done`，结果角色 + `Done`
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入名称为 `Done`、结果也为 `Done` 的两段不同 PCM。
///   2. 在同一工作区读取该播报组合。
///   3. 分别检查名称片段与结果片段。
///
/// 预期结果：
///   - 正断言：名称读取回 [1, 0]。
///   - 正断言：结果读取回 [2, 0]。
///   - 负断言：两段不得相等，避免 key 仅按文本导致名称被结果覆盖。
#[test]
fn test_same_text_name_and_outcome_keep_distinct_cached_pcm_segments() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(4_000);
    let workspace = PathBuf::from("/projects/alpha");
    let name_pcm = pcm16(vec![1, 0]);
    let outcome_pcm = pcm16(vec![2, 0]);
    let mut cache = AgentAnnouncementAudioCache::default();
    cache.insert(
        &workspace,
        "Done",
        "Done",
        name_pcm.clone(),
        outcome_pcm.clone(),
        now,
    );

    let cached = cache
        .lookup(&workspace, "Done", "Done", now + Duration::from_secs(1))
        .expect("名称与结果词相同的终态播报也必须完整命中");

    assert_eq!(
        cached.name.as_ref(),
        &name_pcm,
        "名称角色的 Done 必须保留名称自己的 PCM"
    );
    assert_eq!(
        cached.outcome.as_ref(),
        &outcome_pcm,
        "结果角色的 Done 必须保留结果自己的 PCM"
    );
    assert_ne!(
        cached.name, cached.outcome,
        "名称与结果词相同也不得让两个缓存角色指向同一段错误 PCM"
    );
}

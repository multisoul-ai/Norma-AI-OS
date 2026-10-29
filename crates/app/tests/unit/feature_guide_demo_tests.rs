use super::{should_play_feature_guide_media, FeatureGuideTopic};
use crate::app::video_wallpaper::{VideoFrameMailbox, VideoWallpaperState};
use crate::app::wallpaper_resolvers::resolve_builtin_asset;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// 场景：终版演示在安装页与速查页之间完整呈现，媒体可从 App 资源目录解析。
/// 数据：九段终版录屏各有 MP4 和静态 WebP，共 18 个文件；首尾两页没有视频。
/// 执行：按预期顺序检查主题的资源路径，走生产资源解析器，验证媒体文件签名。
/// 预期：九组素材均真实存在，路径不重复；旧通知视频不能混入终版列表。
#[test]
fn final_demo_assets_are_complete_and_resolvable() {
    let slugs = [
        "voice-command",
        "agent-collaboration",
        "live-node-shortcuts",
        "layout-command-m",
        "chief-of-staff",
        "right-drawer",
        "switch-workspaces",
        "workspace-home",
        "slash-commands",
    ];
    assert_eq!(
        FeatureGuideTopic::ALL.len(),
        slugs.len() + 2,
        "安装页、九段演示、速查页都须保留"
    );
    for topic in [
        FeatureGuideTopic::ALL[0],
        *FeatureGuideTopic::ALL.last().unwrap(),
    ] {
        assert_eq!(topic.video_asset(), None, "非演示页不得附带伪造视频");
        assert_eq!(topic.poster_asset(), None, "非演示页不得附带伪造海报");
    }
    for (topic, slug) in FeatureGuideTopic::ALL[1..10].iter().zip(slugs) {
        let video = format!("feature-guide/demo-{slug}.mp4");
        let poster = format!("feature-guide/demo-{slug}.webp");
        assert_eq!(
            topic.video_asset(),
            Some(video.as_str()),
            "视频必须匹配该主题"
        );
        assert_eq!(
            topic.poster_asset(),
            Some(poster.as_str()),
            "静态回退必须匹配同一段录屏"
        );
        let video =
            std::fs::read(resolve_builtin_asset(&video).expect("终版视频必须可解析")).unwrap();
        let poster =
            std::fs::read(resolve_builtin_asset(&poster).expect("终版静态海报必须可解析")).unwrap();
        assert_eq!(
            video.get(4..8),
            Some(&b"ftyp"[..]),
            "{slug} 必须是完整 MP4，不是 LFS 指针"
        );
        assert_eq!(poster.get(0..4), Some(&b"RIFF"[..]), "{slug} 海报须为 WebP");
        assert_eq!(
            poster.get(8..12),
            Some(&b"WEBP"[..]),
            "{slug} 海报须为 WebP"
        );
        assert!(
            !poster.windows(4).any(|bytes| bytes == b"ANIM"),
            "减少动态效果的海报必须静止"
        );
        assert!(
            !topic.title().starts_with("guide."),
            "标题必须经过国际化解析"
        );
        assert!(!topic.summary().is_empty(), "演示必须有说明");
    }
    assert!(
        resolve_builtin_asset("feature-guide/demo-agent-notifications.mp4").is_none(),
        "旧通知素材必须移出终版包"
    );
}

/// FG-DEMO-3：真实演示只在弹窗可见、允许动态效果且用户未暂停时播放。
///
/// 数据构造（含关键数值的推导过程）：
///   playback gates = visible(1) × reduce_motion(1) × manually_paused(1)
///   explicitly checked combinations = playing(1) + hidden(1) + reduced(1) + paused(1) = 4
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 传入 visible=true、reduce_motion=false、manually_paused=false。
///   2. 分别关闭可见性、开启减少动态效果、开启手动暂停。
///   3. 比较四种组合的纯播放决策，不启动真实解码线程。
///
/// 预期结果：
///   - 正断言：只有完整满足三个条件时允许播放。
///   - 负断言：隐藏、减少动态效果或用户暂停任一成立时均不得播放。
#[test]
fn feature_guide_video_respects_visibility_motion_and_manual_pause() {
    assert!(
        should_play_feature_guide_media(true, false, false),
        "弹窗可见、允许动态效果且用户未暂停时，终版 Demo 应自动播放"
    );
    assert!(
        !should_play_feature_guide_media(false, false, false),
        "弹窗离开可见层后必须停止播放，不能在背景持续解码"
    );
    assert!(
        !should_play_feature_guide_media(true, true, false),
        "开启减少动态效果时必须只显示终版 WebP 海报"
    );
    assert!(
        !should_play_feature_guide_media(true, false, true),
        "用户手动暂停后不得被下一次重绘自动恢复播放"
    );
}

/// FG-DEMO-4：暂停 Feature Guide 视频必须冻结解码，而不是取消可恢复的视频槽。
///
/// 数据构造（含关键数值的推导过程）：
///   active video slots = 1（frame=None，尚未交付首帧也可暂停）
///   control atomics = cancel(1) + paused(1) = 2
///   playback transitions = playing → paused → playing = 2 次状态切换
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个带独立 cancel/paused 原子标志的 Active 低拷贝视频槽。
///   2. 调用 set_paused(true)，读取两个原子标志。
///   3. 调用 set_paused(false)，再次读取 paused 标志，模拟用户恢复播放。
///
/// 预期结果：
///   - 正断言：暂停精确设置 paused，恢复精确清除 paused。
///   - 负断言：暂停不得设置 cancel，否则用户无法从当前解码器恢复播放。
#[test]
fn feature_guide_pause_freezes_without_cancelling_the_video_slot() {
    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let slot = VideoWallpaperState::Active {
        frame: None,
        mailbox: Arc::new(VideoFrameMailbox::default()),
        cancel: cancel.clone(),
        paused: paused.clone(),
        path: PathBuf::from("/tmp/feature-guide.mp4"),
    };

    slot.set_paused(true);

    assert!(
        paused.load(Ordering::Acquire),
        "Feature Guide 暂停后必须设置共享 paused 标志，让后台解码线程停止取帧"
    );
    assert!(
        !cancel.load(Ordering::Acquire),
        "Feature Guide 暂停不得取消视频槽，否则播放控制会退化成重新创建解码器"
    );

    slot.set_paused(false);

    assert!(
        !paused.load(Ordering::Acquire),
        "用户恢复播放后必须清除 paused 标志，让同一视频槽继续解码"
    );
    assert!(
        slot.is_active(),
        "暂停与恢复后视频槽必须仍为 Active，不能误转成 Failed"
    );
}

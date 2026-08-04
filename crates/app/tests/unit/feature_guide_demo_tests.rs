use super::{should_play_feature_guide_media, FeatureGuideTopic};
use crate::app::video_wallpaper::{VideoFrameMailbox, VideoWallpaperState};
use crate::app::wallpaper_resolvers::resolve_builtin_asset;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// FG-DEMO-0：官网新增的语音群控与 Agent 协作 Demo 必须成为 Feature Guide 第 2、3 项。
///
/// 数据构造（含关键数值的推导过程）：
///   orchestration guide = 1
///   new website demos   = voice group control(1) + Agent collaboration(1) = 2
///   existing demos      = shortcut(1) + layout(1) + notification(1) = 3
///   Total               = 1 + 2 + 3 = 6 topics
///   media per new demo  = MP4(1) + WebP poster(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 FeatureGuideTopic::ALL 的总数以及第 1、2、3 项标题。
///   2. 读取第 2 项语音群控的正文、视频与海报路径。
///   3. 读取第 3 项 Agent 协作的正文、视频与海报路径。
///
/// 预期结果：
///   - 正断言：编排保持第 1 项，语音群控与 Agent 协作严格位于第 2、3 项，总数为 6。
///   - 正断言：两项新 Demo 的标题、正文及 MP4/WebP 路径与官网数据一致。
///   - 负断言：旧 Live Node Shortcuts 不得继续占据第 2 项。
#[test]
fn new_voice_and_agent_collaboration_demos_are_second_and_third() {
    assert_eq!(
        FeatureGuideTopic::ALL.len(),
        6,
        "Feature Guide 必须包含编排首屏、两段新 Demo 和原有三段 Demo，共 6 项"
    );
    assert_eq!(
        FeatureGuideTopic::ALL[0].title(),
        "Enable Agent-to-Agent Delegation",
        "Agent Orchestration 必须继续作为 Feature Guide 第 1 项"
    );
    assert_eq!(
        FeatureGuideTopic::ALL[1].title(),
        "Voice group control: direct six Agents at once",
        "官网 Voice group control Demo 必须插入为 Feature Guide 第 2 项"
    );
    assert_ne!(
        FeatureGuideTopic::ALL[1].navigation_label(),
        "Live Node Shortcuts",
        "插入后 Live Node Shortcuts 不得继续占据 Feature Guide 第 2 项"
    );
    assert_eq!(
        FeatureGuideTopic::ALL[1].summary(),
        "One spoken instruction controls six Agents. Norma creates six Agent Live Nodes, gives each one its own task, and keeps every run visible in the same workspace.",
        "语音群控正文必须与官网英文产品叙事一致"
    );
    assert_eq!(
        FeatureGuideTopic::ALL[1].video_asset(),
        Some("feature-guide/demo-voice-command.mp4"),
        "语音群控必须绑定官网桌面版 MP4"
    );
    assert_eq!(
        FeatureGuideTopic::ALL[1].poster_asset(),
        Some("feature-guide/demo-voice-command.webp"),
        "语音群控必须绑定官网 WebP 海报"
    );
    assert_eq!(
        FeatureGuideTopic::ALL[2].title(),
        "Agents delegate tasks to each other",
        "官网 Agent collaboration Demo 必须插入为 Feature Guide 第 3 项"
    );
    assert_eq!(
        FeatureGuideTopic::ALL[2].summary(),
        "One Agent can hand a focused task to another Agent and receive the result inside the same workspace. Work branches without making you relay every instruction yourself.",
        "Agent 协作正文必须与官网英文产品叙事一致"
    );
    assert_eq!(
        FeatureGuideTopic::ALL[2].video_asset(),
        Some("feature-guide/demo-agent-collaboration.mp4"),
        "Agent 协作必须绑定官网 MP4"
    );
    assert_eq!(
        FeatureGuideTopic::ALL[2].poster_asset(),
        Some("feature-guide/demo-agent-collaboration.webp"),
        "Agent 协作必须绑定官网 WebP 海报"
    );
}

/// FG-DEMO-1：功能导览必须在编排首屏之后完整呈现三段正式产品叙事。
///
/// 数据构造（含关键数值的推导过程）：
///   website story demos = shortcut(1) + layout(1) + notification(1) = 3
///   orchestration guide = 1
///   app guide topics    = orchestration(1) + story demos(3) = 4
///   media per topic     = MP4(1) + WebP poster(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 跳过无媒体的编排首屏，按 FeatureGuideTopic::ALL 的稳定顺序读取后三个主题。
///   2. 将英文标题、侧栏短标签、正文和媒体路径逐项比较。
///   3. 排除旧版“语音派发”首屏，确保弹窗不再展示代码绘制的占位叙事。
///
/// 预期结果：
///   - 正断言：编排首屏没有伪造媒体，后三个主题的英文标题、侧栏短标签及其 MP4/WebP 路径逐项匹配产品 Demo。
///   - 正断言：英文正文完整说明快捷键、布局和通知行为。
///   - 负断言：首屏不得继续显示旧版中文标题或语音派发主题。
#[test]
fn feature_guide_topics_match_the_website_product_demos() {
    assert_eq!(
        FeatureGuideTopic::ALL.len(),
        4,
        "Feature Guide 应包含 1 个编排首屏和官网的 3 段产品 Demo"
    );
    let orchestration = FeatureGuideTopic::ALL[0];
    let shortcuts = FeatureGuideTopic::ALL[1];
    let layouts = FeatureGuideTopic::ALL[2];
    let notifications = FeatureGuideTopic::ALL[3];

    assert_eq!(
        orchestration.video_asset(),
        None,
        "编排引导没有产品 Demo，不得绑定伪造视频"
    );
    assert_eq!(
        orchestration.poster_asset(),
        None,
        "编排引导没有产品 Demo，不得绑定伪造海报"
    );

    assert_eq!(
        shortcuts.title(),
        "Jump to Any Live Node",
        "第 1 个主题必须使用简洁的英文 Live Node 标题"
    );
    assert_eq!(
        shortcuts.navigation_label(),
        "Live Node Shortcuts",
        "第 1 个侧栏入口必须使用紧凑的英文导航标签"
    );
    assert_eq!(
        shortcuts.summary(),
        "Press ⌘1–6 to bring a specific Live Node forward instantly. Move between agents, terminals, and previews without hunting through tabs.",
        "第 1 个主题正文必须用英文完整说明 Live Node 快捷切换"
    );
    assert_eq!(
        shortcuts.video_asset(),
        Some("feature-guide/demo-live-node-shortcuts.mp4"),
        "第 1 个主题必须指向官网的 Live Node 快捷键视频"
    );
    assert_eq!(
        shortcuts.poster_asset(),
        Some("feature-guide/demo-live-node-shortcuts.webp"),
        "第 1 个主题必须使用官网配套 WebP 海报"
    );

    assert_eq!(
        layouts.title(),
        "Reflow Your Workspace",
        "第 2 个主题必须使用简洁的英文工作区标题"
    );
    assert_eq!(
        layouts.navigation_label(),
        "Workspace Layouts",
        "第 2 个侧栏入口必须使用紧凑的英文导航标签"
    );
    assert_eq!(
        layouts.summary(),
        "Press ⌘M to switch between focused and overview layouts. Every running Live Node stays visible, active, and connected to its task.",
        "第 2 个主题正文必须用英文完整说明工作区重排"
    );
    assert_eq!(
        layouts.video_asset(),
        Some("feature-guide/demo-layout-command-m.mp4"),
        "第 2 个主题必须指向官网的 Command-M 布局视频"
    );
    assert_eq!(
        layouts.poster_asset(),
        Some("feature-guide/demo-layout-command-m.webp"),
        "第 2 个主题必须使用官网配套 WebP 海报"
    );

    assert_eq!(
        notifications.title(),
        "Know When an Agent Needs You",
        "第 3 个主题必须使用清晰的英文 Agent 通知标题"
    );
    assert_eq!(
        notifications.navigation_label(),
        "Agent Notifications",
        "第 3 个侧栏入口必须使用紧凑的英文导航标签"
    );
    assert_eq!(
        notifications.summary(),
        "Completion and attention indicators appear directly on the relevant Live Node, so you can see which agent changed state without checking every terminal.",
        "第 3 个主题正文必须用英文完整说明 Agent 状态提示"
    );
    assert_eq!(
        notifications.video_asset(),
        Some("feature-guide/demo-agent-notifications.mp4"),
        "第 3 个主题必须指向官网的 Agent 通知视频"
    );
    assert_eq!(
        notifications.poster_asset(),
        Some("feature-guide/demo-agent-notifications.webp"),
        "第 3 个主题必须使用官网配套 WebP 海报"
    );
    assert_ne!(
        shortcuts.title(),
        "一键唤起 Live Node",
        "Feature Guide 首屏不得继续显示旧版中文标题"
    );
    assert_ne!(
        shortcuts.title(),
        "用语音把任务交给 Agent",
        "Feature Guide 首屏不得继续显示旧版代码绘制的语音派发主题"
    );
}

/// FG-DEMO-2：官网三组 MP4/WebP 素材必须能从开发资源目录解析并保持正确文件格式。
///
/// 数据构造（含关键数值的推导过程）：
///   website media copies = videos(3) + posters(3) = 6
///   MP4 signature range  = bytes[4..8] = "ftyp"（4 bytes）
///   WebP signatures      = bytes[0..4] = "RIFF" + bytes[8..12] = "WEBP"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用运行时同一 resolve_builtin_asset 路径分别解析六个内置文件。
///   2. 逐个读取三段视频并检查 MP4 ftyp 标记。
///   3. 逐个读取三张海报并检查 RIFF/WEBP 标记，同时排除不存在的旧占位视频。
///
/// 预期结果：
///   - 正断言：六个官网素材均存在，三段视频是 MP4，三张海报是 WebP。
///   - 负断言：旧版 synthesized-guide.mp4 不得被误解析为新素材。
#[test]
fn website_demo_assets_are_resolvable_mp4_and_webp_files() {
    let shortcut_video = FeatureGuideTopic::ALL[1]
        .video_asset()
        .and_then(resolve_builtin_asset)
        .expect("第 1 段官网 MP4 必须复制到 app 的 feature-guide 资源目录");
    let layout_video = FeatureGuideTopic::ALL[2]
        .video_asset()
        .and_then(resolve_builtin_asset)
        .expect("第 2 段官网 MP4 必须复制到 app 的 feature-guide 资源目录");
    let notification_video = FeatureGuideTopic::ALL[3]
        .video_asset()
        .and_then(resolve_builtin_asset)
        .expect("第 3 段官网 MP4 必须复制到 app 的 feature-guide 资源目录");
    let shortcut_poster = FeatureGuideTopic::ALL[1]
        .poster_asset()
        .and_then(resolve_builtin_asset)
        .expect("第 1 张官网 WebP 必须复制到 app 的 feature-guide 资源目录");
    let layout_poster = FeatureGuideTopic::ALL[2]
        .poster_asset()
        .and_then(resolve_builtin_asset)
        .expect("第 2 张官网 WebP 必须复制到 app 的 feature-guide 资源目录");
    let notification_poster = FeatureGuideTopic::ALL[3]
        .poster_asset()
        .and_then(resolve_builtin_asset)
        .expect("第 3 张官网 WebP 必须复制到 app 的 feature-guide 资源目录");

    let shortcut_video = std::fs::read(shortcut_video).expect("第 1 段官网 MP4 应可读取");
    let layout_video = std::fs::read(layout_video).expect("第 2 段官网 MP4 应可读取");
    let notification_video = std::fs::read(notification_video).expect("第 3 段官网 MP4 应可读取");
    let shortcut_poster = std::fs::read(shortcut_poster).expect("第 1 张官网 WebP 应可读取");
    let layout_poster = std::fs::read(layout_poster).expect("第 2 张官网 WebP 应可读取");
    let notification_poster =
        std::fs::read(notification_poster).expect("第 3 张官网 WebP 应可读取");

    assert_eq!(
        &shortcut_video[4..8],
        b"ftyp",
        "第 1 段官网素材必须保持 MP4 ftyp 文件标记"
    );
    assert_eq!(
        &layout_video[4..8],
        b"ftyp",
        "第 2 段官网素材必须保持 MP4 ftyp 文件标记"
    );
    assert_eq!(
        &notification_video[4..8],
        b"ftyp",
        "第 3 段官网素材必须保持 MP4 ftyp 文件标记"
    );
    assert_eq!(
        (&shortcut_poster[0..4], &shortcut_poster[8..12]),
        (&b"RIFF"[..], &b"WEBP"[..]),
        "第 1 张官网海报必须保持 RIFF/WEBP 文件标记"
    );
    assert_eq!(
        (&layout_poster[0..4], &layout_poster[8..12]),
        (&b"RIFF"[..], &b"WEBP"[..]),
        "第 2 张官网海报必须保持 RIFF/WEBP 文件标记"
    );
    assert_eq!(
        (&notification_poster[0..4], &notification_poster[8..12]),
        (&b"RIFF"[..], &b"WEBP"[..]),
        "第 3 张官网海报必须保持 RIFF/WEBP 文件标记"
    );
    assert!(
        resolve_builtin_asset("feature-guide/synthesized-guide.mp4").is_none(),
        "Feature Guide 不得回退到不存在的旧版合成占位视频"
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
        "弹窗可见、允许动态效果且用户未暂停时，官网 Demo 应自动播放"
    );
    assert!(
        !should_play_feature_guide_media(false, false, false),
        "弹窗离开可见层后必须停止播放，不能在背景持续解码"
    );
    assert!(
        !should_play_feature_guide_media(true, true, false),
        "开启减少动态效果时必须只显示官网 WebP 海报"
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

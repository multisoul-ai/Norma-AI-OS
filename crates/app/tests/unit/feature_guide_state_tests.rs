use super::style::FeatureGuidePalette;
use super::{
    FeatureGuideBlockers, FeatureGuidePresentation, FeatureGuideSettingsState, FeatureGuideState,
    FeatureGuideStatus, FeatureGuideTopic, GUIDE_STATUS_KEY, GUIDE_TOPIC_KEY,
};
use ui::{Theme, ThemeId};

const FEATURE_GUIDE_MIN_SMALL_TEXT_CONTRAST: f32 = 4.5;

fn color_channels(color: u32) -> ([f32; 3], f32) {
    (
        [
            ((color >> 24) & 0xff) as f32 / 255.0,
            ((color >> 16) & 0xff) as f32 / 255.0,
            ((color >> 8) & 0xff) as f32 / 255.0,
        ],
        (color & 0xff) as f32 / 255.0,
    )
}

fn composite_color(foreground: u32, background: u32) -> [f32; 3] {
    let (foreground, alpha) = color_channels(foreground);
    let (background, _) = color_channels(background);
    [
        foreground[0] * alpha + background[0] * (1.0 - alpha),
        foreground[1] * alpha + background[1] * (1.0 - alpha),
        foreground[2] * alpha + background[2] * (1.0 - alpha),
    ]
}

fn relative_luminance(color: [f32; 3]) -> f32 {
    let linear = color.map(|channel| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

fn contrast_ratio(foreground: u32, background: u32) -> f32 {
    let foreground = relative_luminance(composite_color(foreground, background));
    let background = relative_luminance(composite_color(background, 0x000000ff));
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

fn assert_theme_palette_contrast(theme_id: ThemeId, label: &str) {
    let theme = Theme::for_id(theme_id);
    let palette = FeatureGuidePalette::from_theme(theme);
    let panel = (theme.panel_high & 0xffffff00) | 0xff;

    assert!(
        contrast_ratio(palette.supporting_text, panel) >= FEATURE_GUIDE_MIN_SMALL_TEXT_CONTRAST,
        "{label} 的摘要、步骤和未选导航必须达到 4.5:1 小字号文字对比度"
    );
    assert!(
        contrast_ratio(palette.metadata_text, panel) >= FEATURE_GUIDE_MIN_SMALL_TEXT_CONTRAST,
        "{label} 的进度与次要控制仍需达到 4.5:1，不能因层级较低而不可读"
    );
    assert!(
        contrast_ratio(palette.accent_text, panel) >= FEATURE_GUIDE_MIN_SMALL_TEXT_CONTRAST,
        "{label} 的 eyebrow 与编号强调文字必须在模态面板上保持可读"
    );
    assert!(
        contrast_ratio(palette.primary_foreground, palette.primary_background)
            >= FEATURE_GUIDE_MIN_SMALL_TEXT_CONTRAST,
        "{label} 的主按钮默认态必须达到 4.5:1"
    );
    assert!(
        contrast_ratio(palette.primary_foreground, palette.primary_hover)
            >= FEATURE_GUIDE_MIN_SMALL_TEXT_CONTRAST,
        "{label} 的主按钮悬停态必须保持 4.5:1，不能退回浅底白字"
    );
    assert!(
        contrast_ratio(palette.primary_foreground, palette.primary_active)
            >= FEATURE_GUIDE_MIN_SMALL_TEXT_CONTRAST,
        "{label} 的主按钮按下态必须保持 4.5:1"
    );
}

/// FG-THEME-1：六套主题的导览语义颜色必须让所有 10–12px 文字达到 WCAG AA。
///
/// 数据构造（含关键数值的推导过程）：
///   selectable themes = light(3) + dark(3) = 6
///   required text roles = supporting + metadata + accent = 3
///   primary interaction states = default + hover + active = 3
///   minimum contrast = WCAG AA small text 4.5:1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别从六套真实 Theme token 构造 FeatureGuidePalette。
///   2. 将三种小字号文字合成到各自的 panel_high 背景后计算对比度。
///   3. 检查主按钮三种交互背景与同一 on-accent 前景的对比度。
///
/// 预期结果：
///   - 正断言：六套主题的六种关键颜色组合全部不低于 4.5:1。
///   - 负断言：Forest Mint 不得继续使用固定白色主按钮文字。
///   - 负断言：Hover/Active 背景必须保持不透明，不能退回 0x22/0x32 浅色叠层。
#[test]
fn feature_guide_palette_keeps_small_text_readable_in_every_theme() {
    assert_theme_palette_contrast(ThemeId::PaperBlue, "Paper Blue");
    assert_theme_palette_contrast(ThemeId::LinenSage, "Linen Sage");
    assert_theme_palette_contrast(ThemeId::SilverViolet, "Silver Violet");
    assert_theme_palette_contrast(ThemeId::SapphireNight, "Sapphire Night");
    assert_theme_palette_contrast(ThemeId::MidnightViolet, "Midnight Violet");
    assert_theme_palette_contrast(ThemeId::ForestMint, "Forest Mint");

    let forest = FeatureGuidePalette::from_theme(Theme::for_id(ThemeId::ForestMint));
    assert_ne!(
        forest.primary_foreground, 0xffffffff,
        "Forest Mint 的亮薄荷背景不得继续使用固定白字，否则实际对比度只有约 1.9:1"
    );
    assert_eq!(
        forest.primary_hover & 0xff,
        0xff,
        "主按钮 Hover 背景必须保持不透明，不能使用原来的 0x22 alpha"
    );
    assert_eq!(
        forest.primary_active & 0xff,
        0xff,
        "主按钮 Active 背景必须保持不透明，不能使用原来的 0x32 alpha"
    );
}

/// 首次项目导览资格：仅真正的新用户应在首个 Canvas 稳定后自动看到导览。
///
/// 数据构造（含关键数值的推导过程）：
///   fresh recent workspaces = 0
///   existing recent workspaces = 1（只需一个历史项目即可判定不是首次使用）
///   persisted guide records = 0（两者均模拟升级前没有导览设置）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别加载“无最近项目”和“已有最近项目”的默认导览状态。
///   2. 两个状态都收到一次 workspace opened 事件。
///   3. 两个状态都在无阻塞层时收到 Canvas ready 事件。
///
/// 预期结果：
///   - 正断言：新用户进入 Visible，并标记为 InProgress。
///   - 负断言：已有项目的用户仍为 Idle，不能在升级后被自动打扰。
#[test]
fn first_project_auto_guide_is_limited_to_fresh_users() {
    let mut fresh = FeatureGuideState::load(None, None, false);
    let mut existing = FeatureGuideState::load(None, None, true);

    fresh.on_workspace_opened();
    existing.on_workspace_opened();
    fresh.on_canvas_ready(FeatureGuideBlockers::default());
    existing.on_canvas_ready(FeatureGuideBlockers::default());

    assert_eq!(
        fresh.presentation,
        FeatureGuidePresentation::Visible,
        "没有最近项目的新用户应在首个 Canvas ready 后自动看到导览"
    );
    assert_eq!(
        fresh.status,
        FeatureGuideStatus::InProgress,
        "首次自动展示必须从 NotSeen 进入 InProgress，便于下次继续"
    );
    assert_eq!(
        existing.presentation,
        FeatureGuidePresentation::Idle,
        "已有最近项目的用户不应因版本升级突然出现自动导览"
    );
    assert!(
        !existing.is_visible(),
        "既有用户的导览必须保持不可见，Help 手动入口不受此限制"
    );
}

/// 高优先级界面协调：导览在 Settings 等阻塞层出现时暂停，阻塞消失后恢复。
///
/// 数据构造（含关键数值的推导过程）：
///   visible guide = 1
///   blocker flags = settings_open(1) + voice_active(0) + confirmation(0) + error(0)
///   remaining blockers after close = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 手动打开导览，使其立即进入 Visible。
///   2. 同步 Settings 打开状态，导览转入 Suspended。
///   3. 清空全部 blocker，导览恢复 Visible。
///
/// 预期结果：
///   - 正断言：Settings 打开时导览必须暂停而不是与其争抢层级。
///   - 正断言：Settings 关闭后同一次导览可恢复。
///   - 负断言：暂停过程不能误写成 Completed 或 Suppressed。
#[test]
fn guide_suspends_for_priority_overlay_and_resumes_afterward() {
    let mut guide = FeatureGuideState::load(None, None, true);
    guide.open_manually();

    guide.sync_blockers(FeatureGuideBlockers {
        settings_open: true,
        ..FeatureGuideBlockers::default()
    });

    assert_eq!(
        guide.presentation,
        FeatureGuidePresentation::Suspended,
        "Settings 打开时导览必须转为 Suspended，不能叠在设置面板上"
    );
    assert_eq!(
        guide.status,
        FeatureGuideStatus::InProgress,
        "界面暂停只是瞬态，不应改变用户的导览进度语义"
    );

    guide.sync_blockers(FeatureGuideBlockers::default());

    assert_eq!(
        guide.presentation,
        FeatureGuidePresentation::Visible,
        "所有高优先级阻塞消失后，导览应恢复到原来的可见会话"
    );
    assert!(
        guide.status != FeatureGuideStatus::Completed
            && guide.status != FeatureGuideStatus::Suppressed,
        "暂停与恢复不得误判为用户完成或主动跳过导览"
    );
}

/// 首次展示延迟：Canvas ready 时若 Settings 正在显示，导览保持 Pending 并在关闭后出现。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace opened events = 1
///   canvas ready events = 1
///   blockers at ready = settings_open(1)
///   blockers after Settings close = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 新用户状态收到 workspace opened，进入 Pending。
///   2. Canvas ready 时 Settings 仍打开，导览不展示。
///   3. Settings 关闭后同步空 blocker，复用同一次 Pending 请求展示。
///
/// 预期结果：
///   - 正断言：有 blocker 时保持 Pending，关闭后进入 Visible/InProgress。
///   - 负断言：等待过程中不能变成 ClosedForSession 或丢失自动展示机会。
#[test]
fn pending_auto_guide_waits_for_settings_to_close() {
    let mut guide = FeatureGuideState::load(None, None, false);
    guide.on_workspace_opened();
    guide.on_canvas_ready(FeatureGuideBlockers {
        settings_open: true,
        ..FeatureGuideBlockers::default()
    });

    assert_eq!(
        guide.presentation,
        FeatureGuidePresentation::Pending,
        "Canvas ready 时 Settings 打开，导览必须保留 Pending 而不是叠层展示"
    );
    assert_ne!(
        guide.presentation,
        FeatureGuidePresentation::ClosedForSession,
        "高优先级阻塞只是延迟展示，不能等同用户主动关闭"
    );

    guide.sync_blockers(FeatureGuideBlockers::default());

    assert_eq!(
        guide.presentation,
        FeatureGuidePresentation::Visible,
        "Settings 关闭后 Pending 导览应自动恢复为 Visible"
    );
    assert_eq!(
        guide.status,
        FeatureGuideStatus::InProgress,
        "真正展示首屏时才应将用户状态推进为 InProgress"
    );
}

/// 用户决策持久语义：跳过与完成均停止自动展示，但保存不同状态和最后主题。
///
/// 数据构造（含关键数值的推导过程）：
///   guide instances = skip path(1) + complete path(1) = 2
///   selectable topics = orchestration(1) + website demos(5) = 6；测试选中第 6 个 AgentNotifications
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 两个实例都手动打开并选择 AgentNotifications。
///   2. 第一个执行 suppress，第二个执行 complete。
///   3. 读取状态与主题的稳定序列化值。
///
/// 预期结果：
///   - 正断言：跳过保存 suppressed，完成保存 completed。
///   - 正断言：最后主题保存 agent_notifications，便于 Help 下次定位。
///   - 负断言：两条路径都不能继续保持 Visible 或误回到 not_seen。
#[test]
fn skip_and_complete_preserve_distinct_persisted_outcomes() {
    let mut skipped = FeatureGuideState::load(None, None, true);
    let mut completed = FeatureGuideState::load(None, None, true);
    skipped.open_manually();
    completed.open_manually();
    skipped.select_topic(FeatureGuideTopic::AgentNotifications);
    completed.select_topic(FeatureGuideTopic::AgentNotifications);

    skipped.suppress();
    completed.complete();

    assert_eq!(
        skipped.serialized_status(),
        "suppressed",
        "Skip 必须保存为 suppressed，以区别临时关闭和完整完成"
    );
    assert_eq!(
        completed.serialized_status(),
        "completed",
        "最后一步完成必须保存为 completed，后续不再自动展示"
    );
    assert_eq!(
        completed.serialized_topic(),
        "agent_notifications",
        "用户最后查看的 AgentNotifications 主题应稳定序列化供 Help 恢复"
    );
    assert!(
        !skipped.is_visible() && !completed.is_visible(),
        "Skip 与完成都必须关闭当前弹窗，不能继续遮挡画布"
    );
    assert!(
        skipped.serialized_status() != "not_seen" && completed.serialized_status() != "not_seen",
        "用户明确决策后不得回退为 NotSeen，否则会重复自动弹出"
    );
}

/// 设置持久化批次：导览状态和最后主题必须通过同一原子变更批次写入 app Settings。
///
/// 数据构造（含关键数值的推导过程）：
///   persisted fields = status(1) + topic(1) = 2
///   queued snapshots = 1
///   unexpected legacy keys = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用无存储记录的新 SettingsStore 加载导览设置。
///   2. 用户选择顺延后的第 5 个 WorkspaceLayouts 主题并执行 Skip。
///   3. 将当前快照排队并取出一次待写批次。
///
/// 预期结果：
///   - 正断言：同一批次严格包含 status=suppressed 与 topic=workspace_layouts。
///   - 正断言：取出后 running 状态为 true，避免并发写入乱序。
///   - 负断言：批次不得包含第三个未知 key，也不能把 Skip 写成 completed。
#[test]
fn settings_batch_persists_status_and_topic_atomically() {
    let temp = tempfile::tempdir().expect("测试应能创建临时 Settings 目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let (mut guide, mut settings) = FeatureGuideSettingsState::load_from_store(store, false);
    guide.open_manually();
    guide.select_topic(FeatureGuideTopic::WorkspaceLayouts);
    guide.suppress();

    settings.queue(guide);
    let (_store, changes) = settings
        .take_pending_write()
        .expect("排队后的导览快照必须产生一个待写批次");

    assert_eq!(
        changes,
        vec![
            (GUIDE_STATUS_KEY.to_string(), Some("suppressed".to_string())),
            (
                GUIDE_TOPIC_KEY.to_string(),
                Some("workspace_layouts".to_string())
            ),
        ],
        "导览状态和最后主题必须在同一原子批次中按稳定 key 写入"
    );
    assert_eq!(
        changes.len(),
        2,
        "持久化批次只能包含 status 与 topic 两个字段，不能混入未知设置"
    );
    assert!(
        settings.persistence_running(),
        "取出批次后必须标记写入进行中，防止多个后台任务乱序覆盖"
    );
    assert!(
        !changes
            .iter()
            .any(|(_, value)| value.as_deref() == Some("completed")),
        "Skip 路径不得错误持久化为 completed"
    );
}

/// 当前会话关闭：用户关闭导览后，再打开另一个项目也不能自动重新弹出。
///
/// 数据构造（含关键数值的推导过程）：
///   fresh user = 1（无最近项目，因此具备首次自动展示资格）
///   workspace opened events = first project(1) + second project(1) = 2
///   explicit close events = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 新用户打开第一个项目并在 Canvas ready 后看到导览。
///   2. 用户关闭当前会话的导览，使表现层进入 ClosedForSession。
///   3. 同一 App 会话再打开第二个项目，重新发送 workspace opened。
///
/// 预期结果：
///   - 正断言：第二次打开项目后仍保持 ClosedForSession。
///   - 正断言：进度继续保持 InProgress，供下次启动或 Help 手动恢复。
///   - 负断言：第二个项目不能把导览重新改为 Pending 或 Visible。
#[test]
fn close_for_session_survives_opening_another_workspace() {
    let mut guide = FeatureGuideState::load(None, None, false);
    guide.on_workspace_opened();
    guide.on_canvas_ready(FeatureGuideBlockers::default());
    guide.close_for_session();

    guide.on_workspace_opened();

    assert_eq!(
        guide.presentation,
        FeatureGuidePresentation::ClosedForSession,
        "同一 App 会话内打开第二个项目时，用户已关闭的导览必须继续保持 ClosedForSession"
    );
    assert_eq!(
        guide.status,
        FeatureGuideStatus::InProgress,
        "关闭表现层不应丢失 InProgress 进度，永久 Help 仍可继续上次主题"
    );
    assert_ne!(
        guide.presentation,
        FeatureGuidePresentation::Pending,
        "第二个项目不得重新安排自动导览，否则用户会在同一会话被重复打扰"
    );
    assert!(
        !guide.is_visible(),
        "用户已关闭当前会话后，跨项目切换也不能让导览重新可见"
    );
}

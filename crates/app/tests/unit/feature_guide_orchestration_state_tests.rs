use super::controls::GuideButtonKind;
use super::orchestration::{details_button_interaction, guide_badge_label, GUIDED_PROVIDERS};
use super::style::FeatureGuidePalette;
use super::{
    settings_crossfade_opacities, FeatureGuideSettingsTransition, FeatureGuideState,
    FeatureGuideTopic,
};
use norma::orchestration::{AggregateStatus, OrchestrationProvider};
use ui::{Theme, ThemeId};

/// FG-ORCH-STATE-1：Agent 编排必须保持六项 Feature Guide 的首屏，两段新 Demo 紧随其后。
///
/// 数据构造（含关键数值的推导过程）：
///   existing topics = Live Node(1) + Workspace Layouts(1) + Agent Notifications(1) = 3
///   new demo topics = Voice Group Control(1) + Agent Collaboration(1) = 2
///   orchestration topic = 1
///   Total = 3 + 2 + 1 = 6 topics
///   default saved topic = 0（模拟首次打开，没有历史主题）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从没有持久化主题的新用户状态加载 Feature Guide。
///   2. 读取 ALL 中第 1、2、3、6 项和越界的第 7 项。
///   3. 从首屏执行一次 next_topic，并读取稳定序列化值。
///   4. 用 agent_orchestration、voice_group_control 和 agent_collaboration 持久化值分别恢复状态。
///
/// 预期结果：
///   - 正断言：默认主题和第 1 项都是 AgentOrchestration，总数严格为 6。
///   - 正断言：第 2、3 项依次是 VoiceGroupControl、AgentCollaboration，第 6 项仍是 AgentNotifications。
///   - 正断言：Next 从编排进入语音群控，三个新增/调整主题均可稳定持久化恢复。
///   - 负断言：LiveNodeShortcuts 不得占据第 2 项，也不得存在第 7 项。
#[test]
fn feature_guide_orchestration_is_first_and_persists_stable_progress() {
    let mut guide = FeatureGuideState::load(None, None, false);

    assert_eq!(
        FeatureGuideTopic::ALL.len(),
        6,
        "加入两段官网 Demo 后 Feature Guide 必须严格包含 6 项，不能遗漏或重复主题"
    );
    assert_eq!(
        guide.topic,
        FeatureGuideTopic::AgentOrchestration,
        "没有历史进度时必须默认打开第 1 项 Agent Orchestration"
    );
    assert_eq!(
        FeatureGuideTopic::ALL.first().copied(),
        Some(FeatureGuideTopic::AgentOrchestration),
        "Agent Orchestration 必须位于 ALL 第 1 项，进度才能显示 1 / 6"
    );
    assert_eq!(
        FeatureGuideTopic::ALL.get(1).copied(),
        Some(FeatureGuideTopic::VoiceGroupControl),
        "编排首屏后的第 2 项必须是 Voice Group Control"
    );
    assert_ne!(
        FeatureGuideTopic::ALL.get(1).copied(),
        Some(FeatureGuideTopic::LiveNodeShortcuts),
        "插入语音群控后 Live Node Shortcuts 不得继续占据第 2 项"
    );
    assert_eq!(
        FeatureGuideTopic::ALL.get(2).copied(),
        Some(FeatureGuideTopic::AgentCollaboration),
        "第 3 项必须是 Agents delegate tasks to each other"
    );
    assert_eq!(
        FeatureGuideTopic::ALL.get(5).copied(),
        Some(FeatureGuideTopic::AgentNotifications),
        "现有 Agent Notifications 必须保留为第 6 项和最终完成页"
    );
    assert_eq!(
        FeatureGuideTopic::ALL.get(6),
        None,
        "Feature Guide 不得渲染或推进到不存在的第 7 项"
    );

    guide.next_topic();

    assert_eq!(
        guide.topic,
        FeatureGuideTopic::VoiceGroupControl,
        "编排首屏点击 Next 后必须进入第 2 项 Voice Group Control"
    );
    guide.select_topic(FeatureGuideTopic::AgentOrchestration);
    assert_eq!(
        guide.serialized_topic(),
        "agent_orchestration",
        "编排主题必须使用稳定的 agent_orchestration 值持久化"
    );

    let restored = FeatureGuideState::load(Some("in_progress"), Some("agent_orchestration"), true);
    assert_eq!(
        restored.topic,
        FeatureGuideTopic::AgentOrchestration,
        "Help 后续入口必须从持久化值恢复到 Agent Orchestration"
    );
    let restored_voice =
        FeatureGuideState::load(Some("in_progress"), Some("voice_group_control"), true);
    assert_eq!(
        restored_voice.topic,
        FeatureGuideTopic::VoiceGroupControl,
        "第 2 项语音群控必须从稳定的 voice_group_control 值恢复"
    );
    let restored_collaboration =
        FeatureGuideState::load(Some("in_progress"), Some("agent_collaboration"), true);
    assert_eq!(
        restored_collaboration.topic,
        FeatureGuideTopic::AgentCollaboration,
        "第 3 项 Agent 协作必须从稳定的 agent_collaboration 值恢复"
    );
}

/// FG-ORCH-STATE-2：编排组件说明必须按用户意图展开和收起，且不改变导览进度。
///
/// 数据构造（含关键数值的推导过程）：
///   initial disclosure state = collapsed(0)
///   explicit toggles         = expand(1) + collapse(1) = 2
///   topic changes            = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建首次打开的 Feature Guide，并读取说明展开状态。
///   2. 第一次 toggle_orchestration_details，把说明从收起切到展开。
///   3. 第二次 toggle_orchestration_details，把说明从展开切回收起。
///
/// 预期结果：
///   - 正断言：第一次切换后说明展开，第二次切换后说明收起。
///   - 正断言：两次切换期间主题始终保留在 AgentOrchestration。
///   - 负断言：首次打开时说明不得默认占用页面空间，切换不得推进到下一主题。
#[test]
fn feature_guide_orchestration_details_toggle_without_advancing_progress() {
    let mut guide = FeatureGuideState::load(None, None, false);

    assert!(
        !guide.orchestration_details_expanded(),
        "首次打开编排引导时组件说明必须默认收起"
    );

    guide.toggle_orchestration_details();

    assert!(
        guide.orchestration_details_expanded(),
        "点击 Why are these components needed 后必须展开组件说明"
    );
    assert_eq!(
        guide.topic,
        FeatureGuideTopic::AgentOrchestration,
        "展开组件说明不得推进或重置导览主题"
    );

    guide.toggle_orchestration_details();

    assert!(
        !guide.orchestration_details_expanded(),
        "再次点击组件说明入口后必须收起说明"
    );
    assert_ne!(
        guide.topic,
        FeatureGuideTopic::LiveNodeShortcuts,
        "说明展开状态切换不得误进入 Live Node Shortcuts"
    );
}

/// FG-ORCH-STATE-3：编排导航 badge 必须由四个受支持 Provider 的真实状态推导。
///
/// 数据构造（含关键数值的推导过程）：
///   supported providers = Codex(1) + Claude(1) + Cursor(1) + OpenCode(1) = 4
///   excluded providers  = Grok(1)
///   checked snapshots   = checking(1) + all missing(1) + ready(1) + repair(1) = 4
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取编排引导允许展示的 Provider 稳定列表。
///   2. 依次输入检查中、全部未安装、至少一个就绪、存在修复错误四组快照。
///   3. 比较每组快照生成的导航 badge，并确认 Grok 未进入安装引导。
///
/// 预期结果：
///   - 正断言：检查中显示 Checking，全部未安装显示 Set up，错误显示 Needs attention。
///   - 正断言：至少一个 Ready 且没有错误时保持克制，不显示 badge。
///   - 负断言：Grok 不得出现在四个安装引导 Provider 中，badge 不得永久写死。
#[test]
fn feature_guide_orchestration_badge_tracks_supported_provider_health() {
    assert_eq!(
        GUIDED_PROVIDERS.len(),
        4,
        "编排引导必须严格展示四个首版可安装 Provider"
    );
    assert_eq!(
        GUIDED_PROVIDERS[0],
        OrchestrationProvider::Codex,
        "Provider 列表第 1 项必须是 Codex"
    );
    assert_eq!(
        GUIDED_PROVIDERS[3],
        OrchestrationProvider::OpenCode,
        "Provider 列表第 4 项必须是 OpenCode"
    );
    assert!(
        !GUIDED_PROVIDERS.contains(&OrchestrationProvider::Grok),
        "首版不支持安装的 Grok 不得进入 Feature Guide Provider 列表"
    );

    assert_eq!(
        guide_badge_label(true, [AggregateStatus::NotInstalled; 4], false),
        Some("Checking"),
        "共享健康检查进行中时 badge 必须显示 Checking"
    );
    assert_eq!(
        guide_badge_label(false, [AggregateStatus::NotInstalled; 4], false),
        Some("Set up"),
        "四个 Provider 均未就绪时 badge 必须显示 Set up"
    );
    assert_eq!(
        guide_badge_label(
            false,
            [
                AggregateStatus::Ready,
                AggregateStatus::NotInstalled,
                AggregateStatus::NotInstalled,
                AggregateStatus::NotInstalled,
            ],
            false,
        ),
        None,
        "至少一个 Provider Ready 且无错误时不得继续制造 Setup 焦虑"
    );
    assert_eq!(
        guide_badge_label(
            false,
            [
                AggregateStatus::Ready,
                AggregateStatus::NotInstalled,
                AggregateStatus::NeedsRepair,
                AggregateStatus::NotInstalled,
            ],
            true,
        ),
        Some("Needs attention"),
        "任一受支持 Provider 需要修复时 badge 必须升级为 Needs attention"
    );
    assert_eq!(
        guide_badge_label(
            false,
            [
                AggregateStatus::Ready,
                AggregateStatus::NotInstalled,
                AggregateStatus::NeedsRepair,
                AggregateStatus::NotInstalled,
            ],
            false,
        ),
        None,
        "未授权 Provider 的 NeedsRepair 不得误报 Needs attention"
    );
}

/// FG-ORCH-STATE-4：编排 CTA 与组件说明按钮在亮暗主题下必须提供完整交互反馈。
///
/// 数据构造（含关键数值的推导过程）：
///   themes = PaperBlue(light, 1) + SapphireNight(dark, 1) = 2
///   CTA visual states = rest(1) + hover(1) + pressed(1) + focus ring(1) = 4
///   details visual states = rest(1) + hover(1) + pressed(1) + focus ring(1) = 4
///   topic tab stops = six topics(6)
///   keyboard tab positions = primary(1) + topics(6) + CTA(1) + details(1) = CTA(8), details(9)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别从亮色与暗色主题推导 Feature Guide palette。
///   2. 读取 Orchestration Settings CTA 的 rest、hover、pressed 与键盘配置。
///   3. 读取组件说明按钮的 rest、hover、pressed 与键盘配置。
///   4. 比较两个主题下各状态颜色和稳定 Tab 顺序。
///
/// 预期结果：
///   - 正断言：CTA 与说明按钮在两个主题下均有不同的 hover、pressed 反馈。
///   - 正断言：两者都是 Tab stop，并分别位于第 8、9 个位置，由 Button 绘制 focus ring。
///   - 负断言：hover/pressed 不得退化为静态 rest，亮暗主题不得共用硬编码背景。
#[test]
fn feature_guide_orchestration_controls_cover_interaction_states_in_light_and_dark_themes() {
    let light_theme = Theme::for_id(ThemeId::PaperBlue);
    let dark_theme = Theme::for_id(ThemeId::SapphireNight);
    let light_palette = FeatureGuidePalette::from_theme(light_theme);
    let dark_palette = FeatureGuidePalette::from_theme(dark_theme);
    let light_cta = GuideButtonKind::OrchestrationSettings.interaction(light_theme, light_palette);
    let dark_cta = GuideButtonKind::OrchestrationSettings.interaction(dark_theme, dark_palette);
    let light_details = details_button_interaction(light_theme, light_palette);
    let dark_details = details_button_interaction(dark_theme, dark_palette);

    assert_ne!(
        light_cta.background, light_cta.hover,
        "亮色主题 CTA hover 必须与 rest 背景不同"
    );
    assert_ne!(
        light_cta.hover, light_cta.active,
        "亮色主题 CTA pressed 必须与 hover 背景不同"
    );
    assert_ne!(
        dark_cta.background, dark_cta.hover,
        "暗色主题 CTA hover 必须与 rest 背景不同"
    );
    assert_ne!(
        dark_cta.hover, dark_cta.active,
        "暗色主题 CTA pressed 必须与 hover 背景不同"
    );
    assert_ne!(
        light_cta.background, dark_cta.background,
        "亮暗主题 CTA 不得共用与主题无关的硬编码背景"
    );
    assert!(
        light_cta.tab_stop && dark_cta.tab_stop,
        "CTA 在亮暗主题下都必须可通过 Tab 聚焦并显示组件 focus ring"
    );
    assert_eq!(
        (light_cta.tab_index, dark_cta.tab_index),
        (8, 8),
        "六个主题插入 Tab 序列后，CTA 在亮暗主题下必须稳定占据第 8 个位置"
    );
    assert_ne!(
        light_details.background, light_details.hover,
        "亮色主题说明按钮 hover 必须与透明 rest 背景不同"
    );
    assert_ne!(
        light_details.hover, light_details.active,
        "亮色主题说明按钮 pressed 必须与 hover 背景不同"
    );
    assert_ne!(
        dark_details.background, dark_details.hover,
        "暗色主题说明按钮 hover 必须与透明 rest 背景不同"
    );
    assert_ne!(
        dark_details.hover, dark_details.active,
        "暗色主题说明按钮 pressed 必须与 hover 背景不同"
    );
    assert_eq!(
        (light_details.tab_index, dark_details.tab_index),
        (9, 9),
        "六个主题插入 Tab 序列后，说明按钮必须稳定占据第 9 个位置并显示组件 focus ring"
    );
    assert_ne!(
        light_details.foreground, dark_details.foreground,
        "说明按钮文字必须随亮暗主题 palette 变化，不能写死颜色"
    );
}

/// FG-ORCH-STATE-5：Reduced Motion 的 Settings 往返必须是对称纯透明度 crossfade。
///
/// 数据构造（含关键数值的推导过程）：
///   opening samples = start(0.0) + midpoint(0.5) + end(1.0) = 3
///   closing samples = start(0.0) + midpoint(0.5) + end(1.0) = 3
///   surfaces = Feature Guide(1) + Settings(1) = 2
///   spatial displacement = 0px（只允许 opacity 变化）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 计算 Opening 在 0%、50%、100% 的两层 opacity。
///   2. 计算 Closing 在 0%、50%、100% 的两层 opacity。
///   3. 比较每个时间点两层 opacity 总和与首尾对称关系。
///
/// 预期结果：
///   - 正断言：Opening 从 Guide 1/Settings 0 过渡到 Guide 0/Settings 1。
///   - 正断言：Closing 完全反向，且中点两层各为 0.5。
///   - 负断言：任一时刻不得同时全透明，也不得出现 bounce/位移参数。
#[test]
fn feature_guide_settings_reduced_motion_crossfade_is_symmetric_and_opacity_only() {
    let opening_start = settings_crossfade_opacities(FeatureGuideSettingsTransition::Opening, 0.0);
    let opening_mid = settings_crossfade_opacities(FeatureGuideSettingsTransition::Opening, 0.5);
    let opening_end = settings_crossfade_opacities(FeatureGuideSettingsTransition::Opening, 1.0);
    let closing_start = settings_crossfade_opacities(FeatureGuideSettingsTransition::Closing, 0.0);
    let closing_mid = settings_crossfade_opacities(FeatureGuideSettingsTransition::Closing, 0.5);
    let closing_end = settings_crossfade_opacities(FeatureGuideSettingsTransition::Closing, 1.0);

    assert_eq!(
        opening_start,
        (1.0, 0.0),
        "Opening 起点必须保留完整 Guide 并让 Settings 全透明"
    );
    assert_eq!(
        opening_mid,
        (0.5, 0.5),
        "Opening 中点必须由两层各承担一半 opacity"
    );
    assert_eq!(
        opening_end,
        (0.0, 1.0),
        "Opening 终点必须完成到全透明 Guide 和完整 Settings"
    );
    assert_eq!(
        closing_start, opening_end,
        "Closing 起点必须精确承接 Opening 终点，不能发生闪烁"
    );
    assert_eq!(closing_mid, opening_mid, "Opening 与 Closing 中点必须对称");
    assert_eq!(
        closing_end, opening_start,
        "Closing 终点必须恢复完整 Guide 并移除 Settings"
    );
    assert!(
        opening_mid.0 + opening_mid.1 > 0.0,
        "crossfade 中点不得让 Guide 与 Settings 同时全透明"
    );
    assert_ne!(
        opening_start, closing_start,
        "Opening 与 Closing 不得错误使用同一方向的 opacity 时间线"
    );
}

#[path = "../support/run_overview_rows.rs"]
mod fixtures;
use fixtures::sample;

#[path = "../support/drawer_locale.rs"]
mod drawer_locale;
use drawer_locale::expected_copy;

use super::{
    move_overview_cursor, overview_action_keeps_overview, overview_action_starts_project,
    overview_feedback, overview_row_height, overview_tone, present_overview, OverviewAction,
    OverviewCursor, OverviewKey, OverviewRowKind, OverviewStatusTone, ProgressMark,
    OVERVIEW_TEXT_PRIMARY, OVERVIEW_TEXT_SECONDARY,
};
use norma::mobile::{
    RunOverview, RunOverviewAttention, RunOverviewAttentionKind, RunOverviewPresence,
};
use ui::Theme;

/// 场景：概览行把缓存投影写成原型里的三组文案，时长只在给定的当前时刻换算。
///
/// 数据构造：now = 1_800_000_000_000。
///   Bella 已完成 4、共 6 项，当前项「原型交互验收」，开始于 now 之前 18 分钟。
///   18 分钟 = 18 × 60_000 = 1_080_000 毫秒。
///   Rio 没有待办，动作「读取画布」，开始于 now 之前 30 秒，不足 1 分钟。
///   Bruno 待开始。审核、单文件成果、双文件成果、文件行和未加载项目各一行。
///   未加载项目的更新时间是 now 之前 12 分钟 = 720_000 毫秒。
/// 执行过程：用这份投影生成概览行，不重新分组。
/// 预期结果：
///   - 正断言：Bella 为「第 5/6 项 · 原型交互验收 · 已运行 18 分」，分段是 4 完成、1 进行中、1 未开始；
///     单文件成果 direct_diff 为真；设计草稿带「未加载」和「12 分钟前」。
///   - 负断言：Rio 与 Bruno 没有进度条；任何文案不含「%」；未加载审核不能启动项目。
#[test]
fn overview_rows_use_prototype_copy_without_percentages() {
    let now = 1_800_000_000_000;
    let screen = present_overview(&sample(now), &[], now);
    assert_eq!(screen.sections.len(), 3, "必须有三组");
    assert_eq!(
        screen.sections[0].title,
        expected_copy("需要你处理", "Needs attention"),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(screen.sections[0].count, Some(4), "待处理计数等于行数");
    assert_eq!(
        screen.sections[1].title,
        expected_copy("运行中", "Running"),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(screen.sections[1].count, Some(2), "运行中按智能体计数");
    assert_eq!(
        screen.sections[2].title,
        expected_copy("待开始", "Not started"),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(
        screen.sections[2].count,
        Some(1),
        "待开始只数智能体，未加载项目行不计入"
    );
    assert_eq!(
        screen.sections[2].rows.len(),
        2,
        "待开始组尾部跟着未加载项目行"
    );
    let attention = &screen.sections[0].rows;
    assert_eq!(
        attention[0].primary,
        expected_copy("待审核 · 抽屉视觉方案", "Needs approval · 抽屉视觉方案"),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(attention[0].secondary, "Bella · Norma-AI-OS");
    assert_eq!(
        attention[0].action_label,
        expected_copy("定位", "Locate"),
        "产品文案应符合当前界面语言"
    );
    assert!(attention[0].enabled, "已加载的审核可以定位");
    assert_eq!(attention[0].height, 48.0, "待处理行高 48px");
    assert!(
        !overview_action_starts_project(&attention[0].action),
        "定位审核不启动项目"
    );
    assert!(
        overview_action_keeps_overview(&attention[0].action),
        "定位后概览保持显示"
    );
    match &attention[1].action {
        OverviewAction::OpenReview {
            paths, direct_diff, ..
        } => {
            assert_eq!(paths.len(), 1, "单文件成果只有 index.html");
            assert!(*direct_diff, "只有一个文件时直接打开差异");
        }
        other => panic!("成果行应打开变更，实际是 {other:?}"),
    }
    match &attention[2].action {
        OverviewAction::OpenReview {
            paths, direct_diff, ..
        } => {
            assert_eq!(paths.len(), 2, "两个文件的成果保留两份路径");
            assert!(!*direct_diff, "多个文件时停在变更列表");
        }
        other => panic!("第二条成果也应打开变更，实际是 {other:?}"),
    }
    assert_eq!(
        attention[3].primary,
        expected_copy(
            "待查看文件 · index.html",
            "File ready for review · index.html"
        ),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(
        attention[3].secondary, "Bella · Norma-AI-OS",
        "文件行写智能体和项目"
    );
    let agents = &screen.sections[1].rows;
    assert_eq!(
        agents[0].status_label.as_deref(),
        Some(expected_copy("运行中", "Running")),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(
        agents[0].approval_badge.as_deref(),
        Some(expected_copy("1 待审核", "Needs approval: 1")),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(
        agents[0].primary, "调整原型布局与透明材质",
        "主标题是会话名称"
    );
    assert!(agents[0].task_line.is_none(), "没有更新的输入就不画第二行");
    assert_eq!(
        agents[0].project_tag.as_deref(),
        Some("Norma-AI-OS · Bella"),
        "智能体行用项目标签标明归属并保留卡片名"
    );
    assert_eq!(
        agents[0].detail.as_deref(),
        Some(expected_copy(
            "第 5/6 项 · 原型交互验收 · 已运行 18 分",
            "Step 5/6 · 原型交互验收 · Running for 18 min"
        )),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(
        agents[0].progress.as_deref(),
        Some(
            [
                ProgressMark::Done,
                ProgressMark::Done,
                ProgressMark::Done,
                ProgressMark::Done,
                ProgressMark::Current,
                ProgressMark::Todo,
            ]
            .as_slice()
        ),
        "6 段里前 4 段完成，第 5 段进行中"
    );
    assert!(agents[1].progress.is_none(), "没有待办就不画进度条");
    assert_eq!(
        agents[1].detail.as_deref(),
        Some(expected_copy(
            "运行中 · 读取画布 · 已运行不到 1 分",
            "Running · 读取画布 · Running for less than 1 min"
        )),
        "产品文案应符合当前界面语言"
    );
    assert!(agents[1].approval_badge.is_none(), "Rio 没有待审核标记");
    let idle = &screen.sections[2].rows[0];
    assert_eq!(
        idle.status_label.as_deref(),
        Some(expected_copy("待开始", "Not started")),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(
        idle.detail.as_deref(),
        Some(expected_copy("尚未开始", "Not started yet")),
        "产品文案应符合当前界面语言"
    );
    assert!(idle.progress.is_none(), "待开始不画进度条");
    let other = &screen.sections[2].rows[1];
    assert_eq!(other.primary, "设计草稿");
    assert_eq!(
        other.secondary,
        expected_copy(
            "1 个智能体 · 上次状态 空闲 · 12 分钟前 · 未加载",
            "Agents: 1 · Last status: Idle · 12 min ago · Not loaded"
        ),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(
        other.action_label,
        expected_copy("切换", "Switch"),
        "产品文案应符合当前界面语言"
    );
    assert!(
        overview_action_starts_project(&other.action),
        "切换才会进入项目页"
    );
    assert!(
        !overview_action_keeps_overview(&other.action),
        "切换项目离开当前概览上下文"
    );
    assert!(other.prelight.is_none(), "项目摘要没有对应的单张卡片");
    let unloaded = RunOverviewAttention {
        kind: RunOverviewAttentionKind::Approval,
        area_id: "area-other".into(),
        node_id: 9,
        project: "设计草稿".into(),
        agent: "Cara".into(),
        title: "旧方案".into(),
        run_id: "run-9".into(),
        turn_id: "turn-9".into(),
        request_id: None,
        fingerprint: None,
        paths: Vec::new(),
        loaded: false,
    };
    let blocked = super::present_overview(
        &RunOverview {
            attention: vec![unloaded],
            running: Vec::new(),
            idle: Vec::new(),
            unloaded_projects: Vec::new(),
        },
        &[],
        now,
    );
    assert!(
        !blocked.sections[0].rows[0].enabled,
        "未加载项目的审核不能定位"
    );
    assert!(
        !overview_action_starts_project(&blocked.sections[0].rows[0].action),
        "禁用的审核也不启动项目"
    );
    let blob = format!("{screen:?}{blocked:?}");
    assert!(!blob.contains('%'), "概览文案不能出现百分比");
    assert!(
        OVERVIEW_TEXT_PRIMARY >= 11.0 && OVERVIEW_TEXT_SECONDARY >= 11.0,
        "最小字号 11px"
    );
    assert_eq!(overview_row_height(OverviewRowKind::Attention), 48.0);
    assert_eq!(overview_row_height(OverviewRowKind::Project), 48.0);
    assert!(
        overview_row_height(OverviewRowKind::Agent { progress: true })
            > overview_row_height(OverviewRowKind::Agent { progress: false }),
        "带进度条的智能体行比没有进度条的更高"
    );
    assert!(
        overview_row_height(OverviewRowKind::Agent { progress: false }) >= 72.0,
        "无进度条的智能体行要容得下名称、项目标签与状态三行"
    );
}

/// 场景：可停止的 Ego 行用停止任务替换定位。
///
/// 数据构造：节点 3 的 raw_state=ego_working，停止目标绑定智能体 9。
/// 执行过程：把这个绑定交给行投影。
/// 预期结果：动作是停止任务且指向 9；没有绑定时仍是定位。
#[test]
fn stoppable_ego_row_uses_the_existing_stop_action() {
    let now = 1_800_000_000_000;
    let mut overview = sample(now);
    overview.running.truncate(1);
    overview.running[0].raw_state = "ego_working".into();
    overview.running[0].node_id = 3;
    overview.running[0].area_id = "area-current".into();
    overview.running[0].progress = None;
    overview.running[0].presence = RunOverviewPresence::Running;
    let stopped = present_overview(&overview, &[("area-current".into(), 3, 9)], now);
    match &stopped.sections[1].rows[0].action {
        OverviewAction::StopEgo {
            agent_id, node_id, ..
        } => {
            assert_eq!(*node_id, 3, "停止的是 Ego 卡片");
            assert_eq!(*agent_id, 9, "停止绑定的智能体是 9");
        }
        other => panic!("可停止的 Ego 应提供停止任务，实际是 {other:?}"),
    }
    assert_eq!(
        stopped.sections[1].rows[0].action_label,
        expected_copy("停止任务", "Stop task"),
        "产品文案应符合当前界面语言"
    );
    let plain = present_overview(&overview, &[], now);
    assert!(
        matches!(
            plain.sections[1].rows[0].action,
            OverviewAction::LocateAgent { node_id: 3, .. }
        ),
        "没有绑定任务时仍定位卡片"
    );
    assert_ne!(
        plain.sections[1].rows[0].action_label,
        expected_copy("停止任务", "Stop task"),
        "没有可停止任务时不显示停止"
    );
}

/// 场景：方向键、Home、End 和 Enter 在概览行之间移动或执行。
///
/// 数据构造：4 行，下标 0、1、2、3。
/// 执行过程：从没有光标开始，依次向下、到末尾、向上、Home、Enter。
/// 预期结果：落点分别是 0、3、2、0，Enter 执行当前行而不是再移动；空列表没有落点。
#[test]
fn overview_keyboard_moves_between_rows_and_enter_activates() {
    assert_eq!(
        move_overview_cursor(None, 4, OverviewKey::Down),
        Some(OverviewCursor::Move(0)),
        "没有光标时向下进入第一行"
    );
    assert_eq!(
        move_overview_cursor(Some(0), 4, OverviewKey::Down),
        Some(OverviewCursor::Move(1))
    );
    assert_eq!(
        move_overview_cursor(Some(1), 4, OverviewKey::End),
        Some(OverviewCursor::Move(3)),
        "End 到最后一行"
    );
    assert_eq!(
        move_overview_cursor(Some(3), 4, OverviewKey::Down),
        Some(OverviewCursor::Move(3)),
        "末行向下不再越界"
    );
    assert_eq!(
        move_overview_cursor(Some(3), 4, OverviewKey::Up),
        Some(OverviewCursor::Move(2))
    );
    assert_eq!(
        move_overview_cursor(Some(2), 4, OverviewKey::Home),
        Some(OverviewCursor::Move(0)),
        "Home 回到第一行"
    );
    assert_eq!(
        move_overview_cursor(Some(0), 4, OverviewKey::Up),
        Some(OverviewCursor::Move(0)),
        "首行向上不再越界"
    );
    assert_eq!(
        move_overview_cursor(Some(1), 4, OverviewKey::Enter),
        Some(OverviewCursor::Activate(1)),
        "Enter 执行当前行"
    );
    assert_ne!(
        move_overview_cursor(Some(1), 4, OverviewKey::Enter),
        Some(OverviewCursor::Move(1)),
        "Enter 不是再移动一次"
    );
    assert_eq!(
        move_overview_cursor(None, 0, OverviewKey::Down),
        None,
        "没有行时方向键不产生落点"
    );
}

/// 场景：浅色和深色主题的行反馈都能和底色区分开。
///
/// 数据构造：Paper Blue 的强调蓝是 0x2F80ED，Sapphire Night 的强调蓝是 0x54A2FF。
///   悬停透明度 0x2E，按下透明度 0x55。焦点用主题的 focused_stroke，不与悬停填充相同。
/// 执行过程：取两种主题的反馈色，并把悬停色以透明度叠到面板高亮色上。
/// 预期结果：悬停、按下、焦点三者互不相同；叠色后的结果和面板本身的距离大于 20。
#[test]
fn overview_row_feedback_is_distinct_in_light_and_dark_themes() {
    let paper = overview_feedback(Theme::paper_blue());
    assert_eq!(paper.rest, 0x0000_0000, "静息不铺底色");
    assert_eq!(paper.hover, 0x2f80_ed2e, "浅色悬停是 0x2F80ED 的 0x2E");
    assert_eq!(paper.pressed, 0x2f80_ed55, "浅色按下比悬停更深");
    assert_eq!(
        paper.focus,
        Theme::paper_blue().focused_stroke,
        "焦点用描边色"
    );
    assert_ne!(paper.hover, paper.pressed, "按下必须不同于悬停");
    assert_ne!(paper.focus, paper.hover, "焦点不能只靠悬停色");
    let night = overview_feedback(Theme::sapphire_night());
    assert_eq!(night.hover, 0x54a2_ff2e, "深色悬停是 0x54A2FF 的 0x2E");
    assert_eq!(night.pressed, 0x54a2_ff55, "深色按下比悬停更深");
    assert_ne!(night.hover, night.pressed);
    assert_ne!(night.focus, night.hover);
    assert!(
        color_distance(blend(0xfffe_fbd1, paper.hover), opaque(0xfffe_fbd1)) > 20,
        "浅色悬停叠在面板上仍可见"
    );
    assert!(
        color_distance(blend(0x0b1d_2eb3, night.hover), opaque(0x0b1d_2eb3)) > 20,
        "深色悬停叠在面板上仍可见"
    );
    let paper_tone = overview_tone(Theme::paper_blue());
    assert_ne!(
        paper_tone.running, paper_tone.idle,
        "运行中和待开始的颜色不同"
    );
    assert_ne!(
        paper_tone.failed, paper_tone.running,
        "出错和运行中的颜色不同"
    );
    assert_ne!(paper_tone.approval, paper_tone.file, "审核和文件的颜色不同");
    let night_tone = overview_tone(Theme::sapphire_night());
    assert_ne!(night_tone.running, night_tone.idle);
    assert_ne!(night_tone.failed, night_tone.idle);
    let _ = OverviewStatusTone::Running;
}

fn blend(background: u32, foreground: u32) -> (u8, u8, u8) {
    let alpha = (foreground & 0xff) as u32;
    let mix = |shift: u32| {
        let back = (background >> shift) & 0xff;
        let fore = (foreground >> shift) & 0xff;
        ((fore * alpha + back * (255 - alpha)) / 255) as u8
    };
    (mix(24), mix(16), mix(8))
}

fn opaque(color: u32) -> (u8, u8, u8) {
    (
        ((color >> 24) & 0xff) as u8,
        ((color >> 16) & 0xff) as u8,
        ((color >> 8) & 0xff) as u8,
    )
}

fn color_distance(left: (u8, u8, u8), right: (u8, u8, u8)) -> i32 {
    (left.0 as i32 - right.0 as i32).abs()
        + (left.1 as i32 - right.1 as i32).abs()
        + (left.2 as i32 - right.2 as i32).abs()
}

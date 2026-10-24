//! 事件模型、计时器、去抖与时间格式的公开 API 测试。

use analytics::*;

/// 清洗 Token：自由文本只能剩下短小 ASCII 标识。
///
/// 数据构造：把一条含空格、斜杠、中文的“命令/路径”塞进 slug 与 version。
///
/// 预期结果：
///   - 正断言：slug("Theme-Mode") == "theme-mode"（大写转小写）
///   - 负断言：结果不含空格、`/`、中文；长度 <= 48
#[test]
fn token_strips_free_text() {
    let slug = Token::slug("rm -rf /Users/我/secret");
    assert_eq!(slug.as_str(), "rm-rfuserssecret", "非白名单字符应被丢弃");
    assert!(
        !slug.as_str().contains('/') && !slug.as_str().contains(' '),
        "不得残留路径或空格"
    );
    assert_eq!(
        Token::slug("Theme-Mode").as_str(),
        "theme-mode",
        "大写应转小写"
    );
    let long = Token::slug(&"a".repeat(200));
    assert_eq!(long.as_str().len(), 48, "最长 48 个字符");
    assert_eq!(
        Token::version("1.2.3-beta+4").as_str(),
        "1.2.3-beta+4",
        "版本号保留 . - +"
    );
}

/// 事件名与属性：AgentTaskCompleted 只携带枚举、布尔与耗时桶。
///
/// 预期结果：
///   - 正断言：属性键恰为 agent / success / duration_bucket
///   - 负断言：不含任何自由文本字段
#[test]
fn agent_task_completed_has_only_whitelisted_properties() {
    let event = Event::AgentTaskCompleted {
        agent: AgentKind::Claude,
        success: true,
        duration: DurationBucket::from_secs(75),
    };
    assert_eq!(event.name(), "agent_task_completed", "事件名");
    let props = event.properties();
    let mut keys: Vec<_> = props.keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["agent", "duration_bucket", "success"],
        "属性键应只有白名单字段"
    );
    assert_eq!(props["duration_bucket"], "lt_5m", "75s 落在 60..=299 秒桶");
    assert_eq!(props["agent"], "claude", "agent 取枚举字面量");
}

/// 耗时分桶边界。
///
/// 数值推导：9s→lt_10s，10s→lt_1m，59s→lt_1m，60s→lt_5m，299s→lt_5m，
/// 300s→lt_30m，1799s→lt_30m，1800s→ge_30m。
#[test]
fn duration_bucket_boundaries() {
    let cases = [
        (9, "lt_10s"),
        (10, "lt_1m"),
        (59, "lt_1m"),
        (60, "lt_5m"),
        (299, "lt_5m"),
        (300, "lt_30m"),
        (1799, "lt_30m"),
        (1800, "ge_30m"),
    ];
    for (secs, expect) in cases {
        assert_eq!(
            DurationBucket::from_secs(secs).as_str(),
            expect,
            "{secs}s 应落入 {expect}"
        );
    }
}

/// 活跃计时：空闲超过 60 秒的部分不计入活跃。
///
/// 数值推导（毫秒）：前台 t=0；触摸 10s(+10) 20s(+10)；200s 触摸只记 min(180,60)=60；
/// 210s 退后台 +10 → active=90s，foreground=210s。
///
/// 预期结果：
///   - 正断言：(210_000, 90_000)
///   - 负断言：再次 background 返回 None；后台时 touch 不改变结果
#[test]
fn active_timer_caps_idle_gaps() {
    let mut timer = ActiveTimer::default();
    timer.foreground(0);
    timer.touch(10_000);
    timer.touch(20_000);
    timer.touch(200_000);
    assert_eq!(
        timer.background(210_000),
        Some((210_000, 90_000)),
        "前台 210s / 活跃 90s"
    );
    assert_eq!(timer.background(220_000), None, "已在后台，不应重复结算");
    timer.touch(300_000);
    timer.foreground(400_000);
    assert_eq!(
        timer.background(405_000),
        Some((5_000, 5_000)),
        "新一轮从 0 开始计，后台期间的 touch 不残留"
    );
}

/// 去抖：窗口内同键被拦截，不同键与窗口外放行。
///
/// 数据构造：窗口 10_000ms；t=0 放行，t=9_999 拦截，t=10_000 放行。
#[test]
fn debouncer_blocks_repeats_within_window() {
    let mut d = Debouncer::default();
    assert!(d.allow(Feature::Browser, 0, 10_000), "首次放行");
    assert!(
        !d.allow(Feature::Browser, 9_999, 10_000),
        "窗口内重复应拦截"
    );
    assert!(d.allow(Feature::Voice, 9_999, 10_000), "不同功能互不影响");
    assert!(d.allow(Feature::Browser, 10_000, 10_000), "恰好满窗口放行");
}

/// ISO 8601 格式。
///
/// 数值推导：1_700_000_000_123ms = 2023-11-14T22:13:20.123Z；0 = 1970-01-01T00:00:00.000Z；
/// 951_782_400_000ms = 2000-02-29T00:00:00.000Z（闰日）。
#[test]
fn iso8601_formats_utc() {
    assert_eq!(
        iso8601(1_700_000_000_123),
        "2023-11-14T22:13:20.123Z",
        "常规日期"
    );
    assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z", "纪元");
    assert_eq!(iso8601(951_782_400_000), "2000-02-29T00:00:00.000Z", "闰日");
}

/// HTTP 状态分类：瞬时错误重试，其余 4xx 丢弃。
#[test]
fn status_classification() {
    assert_eq!(classify_status(200), SendOutcome::Sent, "2xx 成功");
    assert_eq!(classify_status(429), SendOutcome::Retry, "限流重试");
    assert_eq!(classify_status(503), SendOutcome::Retry, "5xx 重试");
    assert_eq!(
        classify_status(401),
        SendOutcome::Drop,
        "鉴权失败丢弃，避免卡死队列"
    );
    assert_eq!(classify_status(400), SendOutcome::Drop, "坏请求丢弃");
}

use super::live::{EgoCaptureConfig, LatestSequenceMailbox, EGO_CAPTURE_FPS};
use super::pacer::EgoFramePacer;
use super::screencast::{
    parse_screencast_frame_line, parse_screencast_repeat_line, screencast_script,
    EgoScreencastMailbox, EgoScreencastUpdate,
};
use super::worker::EgoWorkerGuard;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Barrier,
};

/// Ego 实时捕获配置：大窗口必须缩到 1920 长边，并以 24 FPS、420f、双缓冲运行。
///
/// 数据构造（含关键数值的推导过程）：
///   原始窗口 = 2_880×1_800 px
///   最大长边 = 1_920 px；缩放比 = 1_920 / 2_880 = 2/3
///   输出高度 = 1_800 × 2/3 = 1_200 px
///   帧率 = 24 FPS；最小帧间隔 = 1 / 24 s ≈ 41.67 ms
///   ScreenCaptureKit 队列深度 = 2，避免单缓冲抖动和多帧积压
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 2_880×1_800 的 Ego 窗口尺寸构造实时捕获配置。
///   2. 读取缩放后的偶数宽高、帧率、队列深度与像素格式。
///   3. 验证配置没有退回 1 FPS、BGRA 或超过 1_920 px 的 CPU 重路径。
///
/// 预期结果：
///   - 正断言：输出为 1_920×1_200、24 FPS、queue depth 2、420f。
///   - 负断言：帧率不得为旧实现的 1 FPS，长边不得超过 1_920 px。
#[test]
fn test_ego_capture_config_targets_24fps_420f_with_bounded_dimensions() {
    let config = EgoCaptureConfig::for_window(2_880, 1_800);

    assert_eq!(
        config.width, 1_920,
        "2_880 px 长边按 2/3 缩放后必须精确限制为 1_920 px"
    );
    assert_eq!(
        config.height, 1_200,
        "1_800 px 短边按 2/3 缩放后必须得到 1_200 px，不能拉伸画面"
    );
    assert_eq!(
        config.frames_per_second, 24,
        "Ego 实时预览必须以 24 FPS 请求 ScreenCaptureKit 帧"
    );
    assert_eq!(
        config.queue_depth, 2,
        "ScreenCaptureKit 必须使用双缓冲，不能让旧帧在系统队列持续积压"
    );
    assert_eq!(
        config.pixel_format_fourcc, *b"420f",
        "实时 Ego 帧必须是 GPUI surface 可直接消费的 420f"
    );
    assert_ne!(
        config.frames_per_second, 1,
        "实时捕获不得回退为旧版每秒一张 PNG 的 1 FPS"
    );
    assert!(
        config.width.max(config.height) <= 1_920,
        "实时捕获输出长边不得超过 1_920 px，实际为 {}×{}",
        config.width,
        config.height
    );
    assert_eq!(
        EGO_CAPTURE_FPS, 24,
        "公开调度常量必须与 ScreenCaptureKit 配置保持同一个 24 FPS 契约"
    );
}

/// Ego 最新帧邮箱：消费者落后时只呈现最新序号，并拒绝晚到的旧帧。
///
/// 数据构造（含关键数值的推导过程）：
///   捕获帧序号 = 40、41、39、42，共 4 次推送
///   第一次消费前有效最新帧 = max(40, 41, 39) = 41
///   乱序帧 39 < 已接收序号 41，必须拒绝
///   第二次消费前新增帧 = 42，因此最新帧 = 42
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 连续推送序号 40 和 41，模拟 GPUI 一次 repaint 之前到达两帧。
///   2. 再推送晚到的序号 39，模拟异步回调乱序。
///   3. 消费一次并再次消费，验证邮箱只交付 41 且随后为空。
///   4. 推送 42 并消费，验证后续新帧仍能继续前进。
///
/// 预期结果：
///   - 正断言：依次交付 41、42，保证画面时间单调前进。
///   - 负断言：40 和 39 不得被呈现，第一次消费后不得残留旧帧。
#[test]
fn test_latest_sequence_mailbox_drops_backlog_and_rejects_out_of_order_frames() {
    let mailbox = LatestSequenceMailbox::default();

    assert!(
        mailbox.push(40, "frame-40"),
        "首帧 40 必须进入空邮箱，建立初始序号"
    );
    assert!(
        mailbox.push(41, "frame-41"),
        "新帧 41 必须替换尚未消费的帧 40"
    );
    assert!(
        !mailbox.push(39, "stale-frame-39"),
        "晚到的旧帧 39 必须被拒绝，不能让画面倒退"
    );

    let first = mailbox.take_latest();
    let empty_after_first = mailbox.take_latest();

    assert_eq!(
        first,
        Some((41, "frame-41")),
        "消费者落后时必须直接拿到最新帧 41，不能先播放积压的 40"
    );
    assert_eq!(
        empty_after_first, None,
        "最新帧 41 被消费后邮箱必须为空，不能残留帧 40 或乱序帧 39"
    );
    assert!(
        mailbox.push(42, "frame-42"),
        "消费完成后的新帧 42 必须继续进入邮箱"
    );
    assert_eq!(
        mailbox.take_latest(),
        Some((42, "frame-42")),
        "后续消费必须得到时间单调递增的帧 42"
    );
    assert_ne!(
        first,
        Some((39, "stale-frame-39")),
        "乱序帧 39 永远不得覆盖或替代帧 41"
    );
}

/// Ego 展示节奏器：新 JPEG 解码较慢时，GPUI 仍按 24 FPS 复用最近有效帧。
///
/// 数据构造（含关键数值的推导过程）：
///   帧间隔 = 42ms，近似 1000 / 24 ≈ 41.67ms
///   首帧到达 = t=0ms；第二次轮询 = t=20ms < 42ms
///   第二个展示点 = t=42ms；新内容到达 = t=50ms
///   第三个展示点 = t=84ms = 42ms + 42ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. t=0 推入 frame-a 并立即轮询，得到展示序号 1。
///   2. t=20 轮询，尚未到下一节奏点，不应重复展示。
///   3. t=42 轮询，在没有新 JPEG 时复用 frame-a，得到序号 2。
///   4. t=50 推入 frame-b；t=84 轮询得到最新内容 frame-b 和序号 3。
///
/// 预期结果：
///   - 正断言：展示序号严格为 1、2、3，且新内容到达后替换旧内容。
///   - 负断言：t=20 不得提前出帧，t=84 不得继续复用已经过期的 frame-a。
#[test]
fn test_frame_pacer_keeps_24fps_cadence_between_slow_jpeg_updates() {
    let started_at = std::time::Instant::now();
    let mut pacer = EgoFramePacer::new(std::time::Duration::from_millis(42));

    pacer.push("frame-a");
    let first = pacer.poll(started_at);
    let early = pacer.poll(started_at + std::time::Duration::from_millis(20));
    let repeated = pacer.poll(started_at + std::time::Duration::from_millis(42));
    pacer.push("frame-b");
    let refreshed = pacer.poll(started_at + std::time::Duration::from_millis(84));

    assert_eq!(
        first,
        Some((1, "frame-a")),
        "首张已解码帧必须立即以展示序号 1 呈现"
    );
    assert_eq!(early, None, "t=20ms 小于 42ms 帧间隔，不得提前产生重复帧");
    assert_eq!(
        repeated,
        Some((2, "frame-a")),
        "没有新 JPEG 时必须在 t=42ms 复用最近有效 frame-a"
    );
    assert_eq!(
        refreshed,
        Some((3, "frame-b")),
        "新内容 frame-b 到达后，t=84ms 必须展示最新帧而不是旧 frame-a"
    );
    assert_ne!(
        refreshed,
        Some((3, "frame-a")),
        "帧节奏器不得在新内容已经到达后继续复用过期 frame-a"
    );
}

/// Ego worker 生命周期：释放守卫必须非阻塞取消，并由 reaper 等待后台线程清理。
///
/// 数据构造（含关键数值的推导过程）：
///   worker 数量 = 1；Drop 调用线程 = 1；reaper 线程 = 1
///   started barrier parties = 测试线程 1 + worker 线程 1 = 2
///   cancel 初值 = false；退出标记初值 = false
///   Drop 非阻塞观察窗 = 50ms = 1ms 轮询 × 最多 50 次
///   worker 退出观察窗 = 1s = 1ms 轮询 × 最多 1000 次
///   清理 release channel = 1；测试在 Drop 返回后才允许 worker 退出
///
/// 执行过程（逐步说明系统如何处理）：
///   1. worker 与测试线程通过 2 方 barrier 确认线程已经运行。
///   2. 独立线程释放 `EgoWorkerGuard` → cancel 由 false 变为 true，JoinHandle 交给 reaper。
///   3. worker 观察到取消，但在 release channel 上等待，模拟 socket/子进程仍在清理。
///   4. 50ms 内确认 Drop 已返回且 worker 仍被阻塞，再放行 worker。
///   5. 最多等待 1s 观察 worker 退出，给 reaper 的异步调度留出确定性窗口。
///
/// 预期结果：
///   - 正断言：Drop 在 worker 清理完成前返回，随后 worker 确实退出。
///   - 负断言：Drop 不得在 GPUI 前台同步 join，也不得丢弃取消后的 worker。
#[test]
fn test_worker_guard_reaps_cancelled_capture_thread_without_blocking_drop() {
    let cancel = Arc::new(AtomicBool::new(false));
    let exited = Arc::new(AtomicBool::new(false));
    let started = Arc::new(Barrier::new(2));
    let drop_returned = Arc::new(AtomicBool::new(false));
    let (release_cleanup, wait_for_cleanup_release) = mpsc::channel();
    let worker_cancel = cancel.clone();
    let worker_exited = exited.clone();
    let worker_started = started.clone();
    let handle = std::thread::spawn(move || {
        worker_started.wait();
        while !worker_cancel.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        wait_for_cleanup_release
            .recv()
            .expect("测试线程必须放行模拟的 socket/helper 清理");
        worker_exited.store(true, Ordering::Release);
    });
    let guard = EgoWorkerGuard::new(cancel, handle);
    started.wait();
    let drop_returned_from_thread = drop_returned.clone();
    let dropper = std::thread::spawn(move || {
        drop(guard);
        drop_returned_from_thread.store(true, Ordering::Release);
    });

    let drop_deadline = std::time::Instant::now() + std::time::Duration::from_millis(50);
    while !drop_returned.load(Ordering::Acquire) && std::time::Instant::now() < drop_deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let returned_before_cleanup = drop_returned.load(Ordering::Acquire);
    assert!(
        returned_before_cleanup,
        "EgoWorkerGuard::drop 必须在 worker 清理完成前返回，不能在 GPUI 前台同步 join"
    );
    assert!(
        !exited.load(Ordering::Acquire),
        "worker 在 release channel 放行前不应退出，否则测试没有覆盖异步清理窗口"
    );
    release_cleanup
        .send(())
        .expect("测试必须放行 worker 完成模拟清理");
    dropper
        .join()
        .expect("EgoWorkerGuard Drop 测试线程不得 panic");

    let exit_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !exited.load(Ordering::Acquire) && std::time::Instant::now() < exit_deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        exited.load(Ordering::Acquire),
        "reaper 必须最终等待后台捕获线程完成，不能遗留 helper 或 Unix socket"
    );
}

/// Ego screencast 协议：只解析带前缀、正整数序号和非空 JPEG 负载的帧行。
///
/// 数据构造（含关键数值的推导过程）：
///   输入行 = 有效 JPEG 1 + 有效重复帧 1 + 普通日志 1 + 缺分隔符 1 + 空负载 1 = 5 行
///   有效序号 = 42；有效负载 = `/9j/4AAQ`，长度 8
///   重复帧序号 = 43；JPEG 负载 = 0 bytes，复用上一张已解码 Arc
///   无效负载数 = 缺分隔符 1 + 空负载 1 = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 `NORMA_EGO_FRAME=42:/9j/4AAQ`，提取序号与 base64 文本。
///   2. 解析 `NORMA_EGO_REPEAT=43`，提取只复用上一张 RenderImage 的序号。
///   3. 解析普通 helper 日志，验证它被安全忽略。
///   4. 解析缺少冒号与空 JPEG 的协议行，验证两者都返回错误。
///
/// 预期结果：
///   - 正断言：JPEG 行得到序号 42 与原始 base64，重复帧得到序号 43，普通日志得到 None。
///   - 负断言：畸形协议不得伪装成帧，也不得把空数据送入图片解码器。
#[test]
fn test_parse_screencast_frame_line_requires_sequence_and_nonempty_jpeg() {
    let valid = parse_screencast_frame_line("NORMA_EGO_FRAME=42:/9j/4AAQ")
        .expect("完整 Ego screencast 协议行必须可解析");
    let repeated =
        parse_screencast_repeat_line("NORMA_EGO_REPEAT=43").expect("完整 Ego 重复帧协议必须可解析");
    let ordinary = parse_screencast_frame_line("ego helper connected")
        .expect("普通 helper 日志必须被安全忽略");
    let missing_separator = parse_screencast_frame_line("NORMA_EGO_FRAME=42");
    let empty_payload = parse_screencast_frame_line("NORMA_EGO_FRAME=42:");

    assert_eq!(
        valid,
        Some((42, "/9j/4AAQ")),
        "有效协议必须精确保留序号 42 与 JPEG base64 负载"
    );
    assert_eq!(
        repeated,
        Some(43),
        "重复帧协议必须只返回序号 43，复用上一张已解码 RenderImage"
    );
    assert_eq!(
        ordinary, None,
        "没有 NORMA_EGO_FRAME 前缀的普通日志不得被误识别为画面"
    );
    assert!(
        missing_separator.is_err(),
        "缺少序号/负载冒号分隔符的协议行必须失败"
    );
    assert!(
        empty_payload.is_err(),
        "空 JPEG 负载必须失败，不能进入图片解码与 GPUI 上传链路"
    );
    assert_ne!(valid, Some((42, "")), "有效帧不得被解析为空 JPEG 数据");
}

/// Ego 隐藏页面脚本：单个持久进程必须启动 CDP screencast、确认每帧并限速到 24 FPS。
///
/// 数据构造（含关键数值的推导过程）：
///   Task Space selector = `23`，共 1 个
///   目标帧率 = 24 FPS；帧间隔表达式 = 1000 / 24 ≈ 41.67 ms
///   隐藏页实测新合成帧 = 27 / 2.5s = 10.8 FPS
///   输出节奏 = 24 FPS；相邻新帧之间重复最近有效帧，不能把 latestFrame 清空
///   everyNthFrame = 1，避免在 10.8 FPS 基础上继续隔帧
///   CDP 生命周期 = startScreencast 1 + screencastFrameAck N + stopScreencast 1
///   socket 终态 = close/error 任一发生即退出循环，共 2 个监听入口
///   禁止的逐帧截图调用 = captureScreenshot 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为 Task Space 23 生成一次长期运行的 Node 脚本。
///   2. 检查脚本启动 Page.startScreencast，并对每个事件发送 frame ack。
///   3. 检查 CDP 接收每一帧，再按 24 FPS 计算 socket 发帧间隔。
///   4. 检查帧发送后仍保留最近有效帧，后台页暂时没有新合成时也维持稳定画面。
///   5. 检查 socket 断开会退出循环并停止 screencast，且没有退回 captureScreenshot。
///
/// 预期结果：
///   - 正断言：脚本包含 Task Space、start/stop、ack、断连退出、最近帧和 24 FPS 节奏协议。
///   - 负断言：脚本不得无限忽略 socket 断连、隔帧或包含逐帧截图与 PNG 临时路径。
#[test]
fn test_screencast_script_is_persistent_acknowledged_and_24fps() {
    let socket_path = std::path::Path::new("/tmp/norma-ego-l3.sock");
    let script = screencast_script("23", socket_path).expect("数字 Task Space ID 必须可编码为脚本");

    assert!(
        script.contains("useOrCreateTaskSpace(taskSelector)"),
        "持久脚本必须只选择一次受控 Task Space"
    );
    assert!(
        script.contains("Page.startScreencast"),
        "隐藏 Ego 页面必须通过 CDP Page.startScreencast 持续产帧"
    );
    assert!(
        script.contains("Page.screencastFrameAck"),
        "每个 CDP screencastFrame 都必须确认，否则 Chromium 只发送少量帧后停住"
    );
    assert!(
        script.contains("socket.once(\"close\"") && script.contains("socket.once(\"error\""),
        "宿主退出或 session 取消时，Node helper 必须观察 socket close/error 并自行收口"
    );
    assert!(
        script.contains("while (!disconnected)"),
        "持久脚本循环必须受 socket 连接状态约束，不能在宿主退出后无限存活"
    );
    assert!(
        script.contains("Page.stopScreencast"),
        "socket 断开后必须显式停止 CDP screencast 再退出 helper"
    );
    assert!(
        !script.contains("while (true)"),
        "持久脚本不得使用忽略宿主断连的无限循环，否则 cargo run 退出后会留下孤儿 helper"
    );
    assert!(
        script.contains("everyNthFrame: 1"),
        "隐藏页可能只以约 30 FPS 合成，CDP 必须接收每一帧再由 24 FPS 输出节流"
    );
    assert!(
        !script.contains("everyNthFrame: 2"),
        "CDP 不得隔帧；30 / 2 ≈ 15 FPS 会低于 22 FPS 验收线"
    );
    assert!(
        script.contains("1000 / 24"),
        "脚本必须把 24 FPS 推导为 1000 / 24 ms 的输出节流"
    );
    assert!(
        script.contains("latestFrame"),
        "脚本必须覆盖尚未输出的旧帧，只向宿主发送最新页面画面"
    );
    assert!(
        script.contains("NORMA_EGO_REPEAT="),
        "两个 Chromium 新帧之间必须发送轻量重复帧协议，不能重复传输和解码同一张 JPEG"
    );
    assert!(
        script.contains("hasFreshFrame"),
        "脚本必须区分新 JPEG 与复用上一张 RenderImage 的节奏帧"
    );
    assert!(
        !script.contains("sequence += 1;\n    latestFrame = null"),
        "后台页新合成只有约 10.8 FPS，发送后必须保留最近有效帧以维持 24 FPS 稳定节奏"
    );
    assert!(
        script.contains("net.createConnection"),
        "无限运行的 helper 会延迟 stdout，脚本必须通过本机 Unix socket 实时推帧"
    );
    assert!(
        script.contains("/tmp/norma-ego-l3.sock"),
        "脚本必须连接宿主创建的精确 Unix socket 路径"
    );
    assert!(
        !script.contains("captureScreenshot"),
        "实时链路不得逐帧调用 captureScreenshot，避免进程启动、PNG 和磁盘开销"
    );
    assert!(
        !script.contains(".png"),
        "持久 screencast 脚本不得生成 PNG 临时文件"
    );
    assert_ne!(
        script.matches("Page.startScreencast").count(),
        0,
        "生成脚本中 startScreencast 调用数量不得为 0"
    );
}

/// Ego 白色过渡帧：单帧质量拒绝不得把持久 CDP session 标记为断开。
///
/// 数据构造（含关键数值的推导过程）：
///   mailbox = 1 个；被拒绝的白帧 = 1 个；底层连接错误 = 0 个
///   可交付正常帧 = 0 个；预期 session 级失败 = 0 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建一个空的 CDP screencast mailbox，模拟已经建立的持久 session。
///   2. 报告一张“量化颜色 1 < 8”的白色过渡帧，模拟页面导航中间态。
///   3. 读取 mailbox 更新，确认本轮既没有新画面，也没有连接级失败。
///
/// 预期结果：
///   - 正断言：mailbox 返回 None，session 可以继续等待下一张正常帧。
///   - 负断言：不得返回 Failed，也不得伪造一张可呈现的白色 Frame。
#[test]
fn test_screencast_mailbox_drops_blank_frame_without_failing_session() {
    let mailbox = EgoScreencastMailbox::default();

    mailbox.reject_frame("Ego CDP screencast未返回可识别的页面内容");
    let update = mailbox.take_update();

    assert!(
        matches!(update, EgoScreencastUpdate::None),
        "白色过渡帧必须只被丢弃，mailbox 应保持空闲并等待下一张正常帧"
    );
    assert!(
        !matches!(update, EgoScreencastUpdate::Failed { .. }),
        "单帧质量错误不得升级为 session 失败，否则会错误切回原生窗口白屏源"
    );
    assert!(
        !matches!(update, EgoScreencastUpdate::Frame { .. }),
        "被质量门拒绝的白帧不得被伪装成可呈现画面"
    );
}

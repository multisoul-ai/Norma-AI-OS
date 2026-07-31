use super::*;

/// Ego 预览源优先级：CDP 主源已启动时，不得再让原生窗口流竞争并覆盖页面帧。
///
/// 数据构造（含关键数值的推导过程）：
///   候选节点 = node 7 + node 8 + node 9 = 3 个
///   CDP 活跃节点 = {7, 9} = 2 个
///   ScreenCaptureKit 后备节点 = 3 - 2 = 1 个，仅 node 8
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造三个可预览 Ego 节点，其中 node 7 和 node 9 已有持久 CDP session。
///   2. 通过次级捕获筛选器计算允许启动 ScreenCaptureKit 的目标。
///   3. 检查结果只保留没有 CDP 主源的 node 8。
///
/// 预期结果：
///   - 正断言：node 8 必须进入原生窗口后备源。
///   - 负断言：node 7 和 node 9 不得进入后备源，避免白色过渡帧覆盖 CDP 页面帧。
#[test]
fn test_secondary_capture_excludes_nodes_with_active_cdp_primary() {
    let targets = vec![
        (7, Some("23".to_string()), "Ego 7".to_string()),
        (8, Some("24".to_string()), "Ego 8".to_string()),
        (9, Some("25".to_string()), "Ego 9".to_string()),
    ];
    let cdp_active = std::collections::HashSet::from([7, 9]);

    let secondary = secondary_capture_targets(&targets, |node_id| cdp_active.contains(&node_id));

    assert_eq!(
        secondary,
        vec![(8, Some("24".to_string()), "Ego 8".to_string())],
        "只有没有 CDP 主源的 node 8 可以启动 ScreenCaptureKit 后备捕获"
    );
    assert!(
        secondary.iter().any(|(node_id, _, _)| *node_id == 8),
        "node 8 缺少 CDP 主源，必须保留一个可用的原生窗口后备源"
    );
    assert!(
        !secondary.iter().any(|(node_id, _, _)| *node_id == 7),
        "node 7 已有 CDP 主源，不得让原生窗口白色过渡帧覆盖页面内容"
    );
    assert!(
        !secondary.iter().any(|(node_id, _, _)| *node_id == 9),
        "node 9 已有 CDP 主源，不得同时运行两个竞争的画面源"
    );
}

/// Ego 内部截图结果解析：只接受带协议前缀的绝对 PNG 路径，忽略 helper 的其它日志。
///
/// 数据构造（含关键数值的推导过程）：
///   helper 输出行数 = 普通日志 2 行 + 协议结果 1 行 = 3 行
///   有效协议路径 = `/var/folders/example/ego-browser-shot-7-1.png`，共 1 个
///   负例总数 = 无协议 1 + 相对路径 1 + 非 PNG 1 = 3 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从混合输出中逐行查找 `NORMA_EGO_SCREENSHOT=` 协议前缀。
///   2. 把协议负载解析为绝对 PNG 路径，并忽略前后普通日志。
///   3. 依次解析无协议、相对路径和非 PNG 输出，验证安全边界全部拒绝。
///
/// 预期结果：
///   - 正断言：返回唯一协议行中的绝对 PNG 路径。
///   - 负断言：无协议、相对路径、非 PNG 都必须失败，且不得返回普通日志文本。
#[test]
fn test_parse_ego_screenshot_path_requires_prefixed_absolute_png() {
    let output = concat!(
        "ego helper connected\n",
        "NORMA_EGO_SCREENSHOT=/var/folders/example/ego-browser-shot-7-1.png\n",
        "capture complete\n",
    );
    let parsed = parse_ego_screenshot_path(output).expect("带协议前缀的绝对 PNG 路径必须可解析");
    let missing = parse_ego_screenshot_path("ego helper connected\ncapture complete\n");
    let relative = parse_ego_screenshot_path("NORMA_EGO_SCREENSHOT=ego-browser-shot-7-1.png\n");
    let non_png = parse_ego_screenshot_path("NORMA_EGO_SCREENSHOT=/tmp/ego-browser-shot-7-1.jpg\n");

    assert_eq!(
        parsed,
        std::path::PathBuf::from("/var/folders/example/ego-browser-shot-7-1.png"),
        "必须返回协议行中的精确截图路径，不能返回普通 helper 日志"
    );
    assert!(
        parsed.is_absolute(),
        "Ego 截图结果必须是绝对路径，避免按 App 当前工作目录误读文件"
    );
    assert_eq!(
        parsed.extension().and_then(std::ffi::OsStr::to_str),
        Some("png"),
        "Ego 截图结果必须保留 PNG 扩展名，供图片解码器确定格式"
    );
    assert!(
        missing.is_err(),
        "没有 NORMA_EGO_SCREENSHOT 协议行时必须失败，不能猜测普通日志"
    );
    assert!(
        relative.is_err(),
        "相对截图路径必须失败，不能让 Ego helper 影响 App 当前工作目录中的任意文件"
    );
    assert!(
        non_png.is_err(),
        "非 PNG 截图路径必须失败，不能把未约定格式交给预览解码链路"
    );
    assert_ne!(
        parsed,
        std::path::PathBuf::from("ego helper connected"),
        "普通连接日志不得被误识别为截图路径"
    );
}

/// Ego 预览目标调度：多个已就绪卡片必须全部进入本轮捕获，旧卡片不得饿死新卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   Ego 卡片总数 = 已就绪旧卡片 1 + 已就绪新卡片 1 + 尚在创建卡片 1 = 3
///   本轮可捕获目标 = 3 - Creating(1) = 2
///   Task Space 名称 = "旧任务"、"当前任务"、"尚未就绪"，各 1 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 依次创建旧卡片、新卡片和仍处于 Creating 的卡片。
///   2. 前两张通过 TaskSpaceReady 进入 ReadyForUser，第三张保持 Creating。
///   3. 收集本轮全部 Ego 预览目标，模拟每秒刷新调度的目标选择阶段。
///
/// 预期结果：
///   - 正断言：旧卡片和新卡片都必须各自携带正确 Task Space 名称进入目标集合。
///   - 负断言：Creating 卡片不得进入目标集合；目标数不得退化为只取第一张的 1 个。
#[test]
fn test_ego_preview_targets_include_every_ready_card() {
    let mut runtime = runtime::Runtime::new();
    let bounds = canvas::CanvasRect::new(0.0, 0.0, 960.0, 640.0);
    let old_node_id = runtime.spawn_ego_browser(Some("旧任务".to_string()), bounds);
    runtime
        .apply_ego_task_command(
            old_node_id,
            runtime::EgoTaskCommand::TaskSpaceReady {
                task_space_id: "old-space".to_string(),
                task_space_name: "旧任务".to_string(),
            },
        )
        .expect("旧 Ego 卡片必须成功进入 ReadyForUser");
    let current_node_id = runtime.spawn_ego_browser(Some("当前任务".to_string()), bounds);
    runtime
        .apply_ego_task_command(
            current_node_id,
            runtime::EgoTaskCommand::TaskSpaceReady {
                task_space_id: "current-space".to_string(),
                task_space_name: "当前任务".to_string(),
            },
        )
        .expect("当前 Ego 卡片必须成功进入 ReadyForUser");
    let creating_node_id = runtime.spawn_ego_browser(Some("尚未就绪".to_string()), bounds);

    let targets = eligible_ego_preview_targets(&runtime);

    assert_eq!(
        targets.len(),
        2,
        "3 张 Ego 卡片减去 1 张 Creating 卡片后，本轮必须调度 2 个预览目标"
    );
    assert!(
        targets.contains(&(
            old_node_id,
            Some("old-space".to_string()),
            "旧任务".to_string()
        )),
        "旧 Ego 卡片仍应继续刷新，不能因新卡片出现而停止"
    );
    assert!(
        targets.contains(&(
            current_node_id,
            Some("current-space".to_string()),
            "当前任务".to_string()
        )),
        "当前 Ego 卡片必须收到预览请求，不能被文档中的旧卡片饿死"
    );
    assert!(
        !targets
            .iter()
            .any(|(node_id, _, _)| *node_id == creating_node_id),
        "Creating 卡片尚无稳定 Ego 窗口，不得进入预览目标集合"
    );
    assert_ne!(
        targets.len(),
        1,
        "预览目标不得退化为 find_map 只取文档第一张 Ego 卡片"
    );
}

/// Ego 预览状态门控：已恢复但控制通道断开的卡片仍应显示只读画面。
///
/// 数据构造（含关键数值的推导过程）：
///   尚未创建窗口的状态 = Creating，共 1 个
///   已有 Task Space 窗口的终态 = Completed + Failed + Disconnected，共 3 个
///   可继续抓取的终态数量 = 3 - 0 = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别把 Creating、Completed、Failed、Disconnected 交给预览状态门控。
///   2. Creating 尚无稳定窗口标题，门控拒绝抓取。
///   3. 其余状态已有持久化 Task Space 名称，门控允许继续显示只读窗口预览。
///
/// 预期结果：
///   - 正断言：Completed、Failed、Disconnected 均允许预览。
///   - 负断言：Creating 不允许预览，避免在窗口尚未生成时持续报错。
#[test]
fn test_ego_preview_allows_restored_disconnected_task_space() {
    let creating = ego_phase_allows_preview(EgoCardPhase::Creating);
    let completed = ego_phase_allows_preview(EgoCardPhase::Completed);
    let failed = ego_phase_allows_preview(EgoCardPhase::Failed);
    let disconnected = ego_phase_allows_preview(EgoCardPhase::Disconnected);

    assert!(
        !creating,
        "Creating 状态尚未生成稳定 Ego 窗口，不得启动预览捕获"
    );
    assert!(completed, "Completed 状态仍应保留 Ego 窗口的只读页面预览");
    assert!(failed, "Failed 状态仍应显示 Ego 窗口，便于用户检查失败现场");
    assert!(
        disconnected,
        "重启后恢复为 Disconnected 的旧卡片必须继续显示对应 Ego 页面"
    );
}

/// Ego 窗口选择：同名历史空窗与其它进程冒名窗口存在时，只选择当前活动的目标窗口。
///
/// 数据构造（含关键数值的推导过程）：
///   候选窗口 = 正确 Ego 窗口 2 个 + 错误进程 1 个 + 错误标题 1 个 = 4 个
///   精确 owner/title 匹配 = 历史空窗(id=21) + 当前活动窗(id=22) = 2 个
///   活动且在屏幕上的精确匹配 = id=22，共 1 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 按 Ego bundle id 与 Task Space 精确标题过滤，排除冒名进程和其它 Task Space。
///   2. 在两个同名 Ego 窗口中优先选择 active=true 的当前窗口。
///   3. 返回可交给窗口捕获器的精确 window_id。
///
/// 预期结果：
///   - 正断言：返回当前活动窗口 id=22。
///   - 负断言：不得返回历史空窗 id=21、冒名窗口 id=31 或错误标题窗口 id=41。
#[test]
fn test_select_ego_window_prefers_active_exact_match() {
    let candidates = vec![
        EgoWindowCandidate::new(
            21,
            Some("com.citrolabs.ego.lite"),
            Some("ego lite"),
            Some("Norma · Ego"),
            false,
            false,
        ),
        EgoWindowCandidate::new(
            22,
            Some("com.citrolabs.ego.lite"),
            Some("ego lite"),
            Some("Norma · Ego"),
            true,
            true,
        ),
        EgoWindowCandidate::new(
            31,
            Some("example.lookalike"),
            Some("ego lite"),
            Some("Norma · Ego"),
            true,
            true,
        ),
        EgoWindowCandidate::new(
            41,
            Some("com.citrolabs.ego.lite"),
            Some("ego lite"),
            Some("其它 Task Space"),
            true,
            true,
        ),
    ];

    let selected =
        select_ego_window_id(&candidates, "Norma · Ego").expect("应找到唯一优先级最高的 Ego 窗口");

    assert_eq!(
        selected, 22,
        "同名窗口存在时必须返回 active=true 的当前 Ego 窗口 id=22"
    );
    assert_ne!(
        selected, 21,
        "历史同名空窗 id=21 不得覆盖当前活动的 Ego 内容"
    );
    assert_ne!(
        selected, 31,
        "其它进程即使伪造相同标题也不得被当成 Ego 窗口"
    );
    assert_ne!(
        selected, 41,
        "同一 Ego 进程中的其它 Task Space 不得成为本卡片预览来源"
    );
}

/// 旧版同名 Ego 窗口兼容：只有一个候选含真实页面像素时选择它，多个非空候选必须拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   空白历史窗 = id 21，量化颜色 2 种
///   实际页面窗 = id 22，量化颜色 45 种
///   非空阈值 = 8 种；2 < 8，45 ≥ 8
///   冲突负例 = id 22(45) + id 23(31)，非空候选共 2 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对旧版同名窗口的内容区做均匀采样并传入颜色多样性分数。
///   2. 从 [空白, 实际页面] 中选择唯一达到阈值的窗口。
///   3. 再传入两个都达到阈值的窗口，验证安全拒绝。
///
/// 预期结果：
///   - 正断言：唯一非空窗口返回 id=22。
///   - 负断言：不得返回空白 id=21；两个非空窗口不得猜测其一。
#[test]
fn test_select_unique_non_blank_legacy_capture_rejects_rich_ambiguity() {
    let selected = select_unique_non_blank_capture(&[(21, 2), (22, 45)])
        .expect("一个非空候选存在时必须兼容旧版同名窗口");
    let ambiguous = select_unique_non_blank_capture(&[(22, 45), (23, 31)]);

    assert_eq!(
        selected, 22,
        "旧版同名窗口必须选择唯一达到颜色阈值的实际页面 id=22"
    );
    assert_ne!(
        selected, 21,
        "颜色种类低于阈值的历史空白窗口 id=21 不得成为预览来源"
    );
    assert!(
        ambiguous.is_err(),
        "两个同名窗口都有实际页面内容时必须拒绝猜测，避免展示错误 Task Space"
    );
    assert_ne!(
        ambiguous,
        Ok(22),
        "多非空候选时不得静默选择第一个窗口 id=22"
    );
}

/// Ego 预览调度：同一节点只允许一个后台捕获，并把刷新率限制为每秒一帧。
///
/// 数据构造（含关键数值的推导过程）：
///   预览间隔 = 1_000ms
///   提前重试点 = 999ms = 1_000ms - 1ms
///   到期重试点 = 1_000ms
///   节点 = node_id 7，共发起 4 次 begin_request
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 t=0 首次请求，登记 inflight。
///   2. 在 t=0 再次请求，因同节点正在捕获而拒绝。
///   3. 标记首轮失败，在 t=999ms 重试，因未满 1 秒而拒绝。
///   4. 在 t=1_000ms 重试，允许启动下一帧捕获。
///
/// 预期结果：
///   - 正断言：t=0 首次请求和 t=1_000ms 到期请求均获准。
///   - 负断言：并发请求与 t=999ms 提前请求均不得获准。
#[test]
fn test_preview_store_throttles_one_fps_and_deduplicates_inflight() {
    let mut store = EgoPreviewStore::default();
    let started_at = std::time::Instant::now();

    let first = store.begin_request(7, started_at);
    let duplicate = store.begin_request(7, started_at);
    store.fail_request(7, first.expect("t=0 首次请求必须返回 generation"));
    let early = store.begin_request(7, started_at + std::time::Duration::from_millis(999));
    let due = store.begin_request(7, started_at + std::time::Duration::from_millis(1_000));

    assert!(
        duplicate.is_none(),
        "同一节点已有捕获任务时不得启动第二个并发任务"
    );
    assert!(
        early.is_none(),
        "距上一帧仅 999ms 时不得提前启动捕获，必须守住 1 FPS"
    );
    assert!(
        due.is_some(),
        "距上一帧达到 1_000ms 时必须允许下一次预览捕获"
    );
    assert_ne!(due, duplicate, "到期请求必须与被拒绝的并发请求产生不同结果");
}

/// L3 Ego 真机页面捕获：按 Task Space ID 抓到的必须是包含实际页面像素的单帧预览。
///
/// 数据构造（含关键数值的推导过程）：
///   Task Space ID = `NORMA_EGO_L3_TASK_SPACE_ID` 环境变量
///   Task Space 标题 = `NORMA_EGO_L3_TASK_SPACE_NAME` 环境变量
///   最小可用尺寸 = 400×300 = 120_000 pixels
///   像素采样上限 ≈ 2_000 个；最低颜色种类 = 16
///   纯白空窗颜色种类 = 1，明显低于 16
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 按精确 Task Space ID 调用 Ego 内部 captureScreenshot，直接读取隐藏页面像素。
///   2. 若内部捕获失败，再按精确标题回退到 macOS 窗口捕获。
///   3. 读取帧尺寸与均匀采样后的 RGB 颜色集合。
///
/// 预期结果：
///   - 正断言：预览恰好一帧，尺寸至少为 400×300，颜色种类不少于 16。
///   - 负断言：不得返回无字节图、纯白/纯色空窗或错误的小尺寸窗口。
#[test]
#[ignore = "需要正在运行且可由 ego-browser 读取的 Ego Lite Task Space"]
fn l3_real_ego_window_capture_returns_non_blank_page_pixels() {
    let task_space_id = std::env::var("NORMA_EGO_L3_TASK_SPACE_ID")
        .expect("L3 必须通过 NORMA_EGO_L3_TASK_SPACE_ID 指定只读目标空间");
    let task_space_name = std::env::var("NORMA_EGO_L3_TASK_SPACE_NAME")
        .expect("L3 必须通过 NORMA_EGO_L3_TASK_SPACE_NAME 指定只读目标窗口");

    let preview = capture_ego_preview_frame(7, 1, Some(&task_space_id), &task_space_name)
        .expect("L3 必须从精确 Ego Task Space 抓取并解码一帧");
    let size = preview.size(0);
    let bytes = preview
        .as_bytes(0)
        .expect("L3 Ego 单帧预览必须暴露第 0 帧像素");
    let pixel_count = bytes.len() / 4;
    // sample_pixels ≈ 2_000；stride_pixels = max(pixel_count / 2_000, 1)。
    let stride_pixels = (pixel_count / 2_000).max(1);
    let colors: std::collections::HashSet<[u8; 3]> = bytes
        .chunks_exact(4)
        .step_by(stride_pixels)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();

    assert_eq!(
        preview.frame_count(),
        1,
        "Ego 窗口预览必须只保留当前一帧，不能积累为动画"
    );
    assert!(
        u32::from(size.width) >= 400 && u32::from(size.height) >= 300,
        "Ego 页面预览必须至少为 400×300，实际为 {}×{}",
        u32::from(size.width),
        u32::from(size.height)
    );
    assert!(!bytes.is_empty(), "Ego 页面预览不得返回空像素缓冲区");
    assert!(
        colors.len() >= 16,
        "Ego 页面预览必须包含实际页面颜色，纯白/纯色空窗会低于 16 种，实际为 {}",
        colors.len()
    );
    assert_ne!(colors.len(), 1, "Ego 页面预览不得退化为单一颜色的空白窗口");
}

/// L3 Ego 真机白帧防护：内部页面与窗口都处于纯白过渡态时，不得产出可显示预览。
///
/// 数据构造（含关键数值的推导过程）：
///   Task Space ID = `NORMA_EGO_L3_BLANK_TASK_SPACE_ID` 环境变量
///   Task Space 标题 = `NORMA_EGO_L3_BLANK_TASK_SPACE_NAME` 环境变量
///   纯白页面量化颜色 = 1 种；最低可见阈值 = 8；1 < 8
///   捕获来源 = Ego 内部页面 1 个 + macOS 精确窗口回退 1 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对预先注入纯白 DOM 的受控 Task Space 调用完整预览捕获链路。
///   2. 内部页面截图经过统一质量门后被拒绝，再回退到精确窗口截图。
///   3. 窗口内容同样未达到颜色阈值，最终返回可诊断错误而不是 RenderImage。
///
/// 预期结果：
///   - 正断言：结果为错误，错误包含“未返回可识别的页面内容”。
///   - 负断言：不得返回可显示帧，也不得把错误误报成 Task Space 未找到。
#[test]
#[ignore = "需要正在运行且已注入纯白页面的 Ego Lite Task Space"]
fn l3_real_ego_preview_rejects_transient_white_page() {
    let task_space_id = std::env::var("NORMA_EGO_L3_BLANK_TASK_SPACE_ID")
        .expect("白帧 L3 必须通过 NORMA_EGO_L3_BLANK_TASK_SPACE_ID 指定受控空间");
    let task_space_name = std::env::var("NORMA_EGO_L3_BLANK_TASK_SPACE_NAME")
        .expect("白帧 L3 必须通过 NORMA_EGO_L3_BLANK_TASK_SPACE_NAME 指定窗口");

    let result = capture_ego_preview_frame(7, 2, Some(&task_space_id), &task_space_name);
    let message = result
        .err()
        .expect("纯白页面与纯白窗口都不得生成可显示 Ego 预览帧");

    assert!(
        message.contains("未返回可识别的页面内容"),
        "纯白真机捕获必须由质量门拒绝，实际错误为：{message}"
    );
    assert!(
        message.contains("量化颜色"),
        "白帧错误必须包含颜色质量诊断，便于区分权限或解码故障，实际为：{message}"
    );
    assert!(
        !message.contains("未找到标题"),
        "受控 Task Space 明明存在，白帧不得被误报为窗口未找到，实际为：{message}"
    );
    assert!(
        !message.is_empty(),
        "白帧拒绝必须返回可诊断错误，不能以空消息结束捕获"
    );
}

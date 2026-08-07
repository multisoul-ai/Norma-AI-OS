use super::*;

/// Ego 预览白帧质量门：过渡期纯白抓图必须被拒绝，真实页面像素必须被接受。
///
/// 数据构造（含关键数值的推导过程）：
///   纯白帧 = 64×64 = 4_096 pixels，所有 RGB 都量化为 `[15, 15, 15]`，颜色分数 = 1
///   页面帧 = 64×64 = 4_096 pixels，RGB 随 x/y 变化，量化颜色分数 > 8
///   最低可见阈值 = 8；纯白 1 < 8，页面帧 > 8
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 把纯白 RGBA 帧交给统一预览质量门，模拟 Ego 页面加载中的瞬时白屏。
///   2. 把含二维颜色变化的 RGBA 帧交给同一质量门，模拟已经完成绘制的网页。
///   3. 比较两次判定，确保质量门不会把“成功返回图片”误当成“成功返回内容”。
///
/// 预期结果：
///   - 正断言：真实页面帧通过质量门，返回不少于 8 的颜色分数。
///   - 负断言：纯白帧被拒绝，错误明确指出没有可识别的页面内容。
#[test]
fn test_validate_ego_preview_rejects_transient_white_frame() {
    let blank = RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]));
    let page = RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([(x * 4) as u8, (y * 4) as u8, ((x + y) * 2) as u8, 255])
    });

    let blank_result = validate_ego_preview_rgba("Ego 页面捕获", &blank);
    let page_result = validate_ego_preview_rgba("Ego 页面捕获", &page);

    assert!(
        blank_result.is_err(),
        "纯白过渡帧只有 1 种量化颜色，必须被质量门拒绝，不能覆盖上一张正常预览"
    );
    assert!(
        blank_result
            .as_ref()
            .is_err_and(|message| message.contains("未返回可识别的页面内容")),
        "纯白帧错误必须解释为没有可识别页面内容，实际为 {blank_result:?}"
    );
    assert!(
        page_result.is_ok(),
        "含真实页面像素的帧必须通过质量门，不能因白帧防护而误伤"
    );
    assert!(
        page_result.is_ok_and(|score| score >= 8),
        "真实页面帧的量化颜色分数必须达到最低阈值 8"
    );
}

/// Ego 预览连续刷新：后续白帧作为捕获错误时，上一张正常帧必须继续可见。
///
/// 数据构造（含关键数值的推导过程）：
///   节点 = node_id 7
///   首次请求时间 = t=0ms；正常预览帧 = 32×32 = 1_024 pixels
///   第二次请求时间 = t=1_000ms，恰好达到 1 FPS 的最短刷新间隔
///   第二次结果 = 白帧质量门产生的错误 1 个，新正常帧 = 0 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 t=0 开始并完成首次请求，把正常帧存入节点 7。
///   2. 在 t=1_000ms 开始第二次请求，模拟质量门把白帧转成捕获错误。
///   3. 读取节点 7 的可见帧、错误和 inflight 状态，验证刷新失败只更新诊断状态。
///
/// 预期结果：
///   - 正断言：第二次错误被记录，且可见帧仍与首次正常帧是同一个 Arc。
///   - 负断言：节点不得丢失可见帧，也不得遗留 inflight 请求。
#[test]
fn test_preview_store_preserves_last_good_frame_after_blank_capture_error() {
    let mut store = EgoPreviewStore::default();
    let node_id = 7;
    let started_at = std::time::Instant::now();
    let first_frame = render_ego_preview_from_rgba(RgbaImage::from_pixel(
        32,
        32,
        image::Rgba([24, 96, 192, 255]),
    ));

    let first_generation = store
        .begin_request(node_id, started_at)
        .expect("t=0 首次请求必须返回 generation");
    let released = store.complete_request(node_id, first_generation, first_frame.clone());
    // retry_at = t=0ms + EGO_PREVIEW_INTERVAL(1_000ms) = t=1_000ms。
    let retry_at = started_at + std::time::Duration::from_millis(1_000);
    let retry_generation = store
        .begin_request(node_id, retry_at)
        .expect("t=1_000ms 已达到刷新间隔，必须允许第二次请求");
    let recorded = store.record_error(
        node_id,
        retry_generation,
        "Ego 页面捕获未返回可识别的页面内容".to_string(),
    );
    let visible_frame = store.frame_for(node_id);
    let visible_error = store.error_for(node_id);

    assert!(
        released.is_empty(),
        "首次正常帧之前没有旧帧，complete_request 不应释放任何可见帧"
    );
    assert!(
        recorded,
        "第二次白帧错误必须被记录，以便日志解释本轮刷新失败"
    );
    assert!(
        visible_frame
            .as_ref()
            .is_some_and(|frame| Arc::ptr_eq(frame, &first_frame)),
        "白帧错误后必须继续显示首次正常帧，不能把画布卡片清空或替换为白色"
    );
    assert!(
        visible_error
            .as_deref()
            .is_some_and(|message| message.contains("未返回可识别的页面内容")),
        "白帧错误必须保留可诊断原因，实际为 {visible_error:?}"
    );
    assert!(
        !store.inflight.contains_key(&node_id),
        "记录白帧错误后必须结束节点 7 的 inflight 请求，避免永久停止刷新"
    );
    assert!(
        visible_frame.is_some(),
        "第二次捕获失败不得删除节点 7 已有的正常预览帧"
    );
}

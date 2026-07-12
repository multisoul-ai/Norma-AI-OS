use super::*;

/// TTS-MANAGER-1：取消中的模型不能继续被展示或使用为 Ready。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog entries = 2（精选 Matcha 与 Kokoro）
///   download total  = 100 bytes（便于观察 40% 的已下载进度）
///   target entry    = 1（第一个 Matcha 条目）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从内置目录构造未扫描状态，读取第一与第二个条目；
///   2. 把第一条推进到 Installing(40 / 100)；
///   3. 请求取消第一条下载，再模拟一个迟到的 90 / 100 进度事件；
///   4. 重新读取两个条目的状态。
///
/// 预期结果：
///   - 断言 A：第一条必须保持 Cancelling，迟到进度不能将其复活为 Installing 或 Ready；
///   - 断言 B：第二条必须保持 NotInstalled，取消一项不能污染另一项。
#[test]
fn test_cancelling_one_tts_install_never_marks_any_entry_ready() {
    let catalog = tts::builtin_model_catalog();
    let first_id = catalog.entries()[0].id().clone();
    let second_id = catalog.entries()[1].id().clone();
    let mut state = TtsModelManagerState::unscanned(&catalog);

    state.begin_install(&first_id, 100);
    state.update_progress(&first_id, 40, 100);
    state.begin_cancellation(&first_id);
    state.update_progress(&first_id, 90, 100);

    assert!(
        matches!(
            state.installation(&first_id),
            Some(TtsModelInstallState::Cancelling {
                downloaded: 40,
                total: 100
            })
        ),
        "取消后的迟到进度不得覆盖 40/100 的 Cancelling 状态或把下载复活"
    );
    assert!(
        matches!(
            state.installation(&second_id),
            Some(TtsModelInstallState::NotInstalled)
        ),
        "取消第一条下载不得把未操作的第二条标记为 Installing、Cancelling 或 Ready"
    );
}

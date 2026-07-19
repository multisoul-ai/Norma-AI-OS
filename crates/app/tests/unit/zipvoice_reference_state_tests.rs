use super::super::{EnvVoiceSettings, SavedVoiceSettings};
use super::*;

/// ZIPVOICE-STATE-1：只有本地 WAV 路径和非空准确转写同时存在时才能开始导入。
///
/// 数据构造（含关键数值的推导过程）：
///   path fields       = 1 个非空本地 WAV 路径
///   transcript cases = 3 个（空字符串、仅空白、准确中文转写）
///   accepted cases   = 1 个（路径 + 准确转写）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 仅填写 WAV 路径，检查导入资格并尝试开始导入；
///   2. 填写仅含空白的转写，再次检查并尝试开始导入；
///   3. 填写非空准确转写，检查资格并开始导入；
///   4. 读取提交快照和 importing 状态，确认合法输入被原样接收。
///
/// 预期结果：
///   - 正断言：路径与准确转写同时存在时允许导入，并进入 importing；
///   - 负断言：空或仅空白转写不得开始导入，也不得进入 importing。
#[test]
fn test_zipvoice_reference_import_requires_path_and_exact_transcript() {
    let model_root = tempfile::tempdir().expect("测试应创建隔离的 Voice 模型根目录");
    let mut state = VoiceSettingsState::from_sources_with_model_root(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
        model_root.path().to_path_buf(),
    );
    state.set_zipvoice_reference_draft_path("/tmp/reference.wav".to_string(), false);

    assert!(
        !state.zipvoice_reference_can_import(),
        "仅有 WAV 路径而缺少准确转写时不得允许 ZipVoice 导入"
    );
    assert!(
        state.begin_zipvoice_reference_import().is_none(),
        "空转写不得产生 ZipVoice 导入提交快照"
    );
    assert!(
        !state.zipvoice_reference_importing(),
        "空转写被拒绝后不得错误进入 importing 状态"
    );

    state.set_zipvoice_reference_draft_transcript("   ".to_string());
    assert!(
        !state.zipvoice_reference_can_import(),
        "仅含空白的转写 trim 后仍不得允许 ZipVoice 导入"
    );
    assert!(
        state.begin_zipvoice_reference_import().is_none(),
        "仅空白转写不得产生 ZipVoice 导入提交快照"
    );
    assert!(
        !state.zipvoice_reference_importing(),
        "仅空白转写被拒绝后不得错误进入 importing 状态"
    );

    state.set_zipvoice_reference_draft_transcript("这段文字与参考声音完全一致。".to_string());
    assert!(
        state.zipvoice_reference_can_import(),
        "WAV 路径与准确转写同时存在时必须允许 ZipVoice 导入"
    );
    let (_, path, transcript) = state
        .begin_zipvoice_reference_import()
        .expect("合法路径与准确转写应产生 ZipVoice 导入提交快照");
    assert_eq!(
        path, "/tmp/reference.wav",
        "合法导入必须保留用户填写的 WAV 路径"
    );
    assert_eq!(
        transcript, "这段文字与参考声音完全一致。",
        "合法导入必须保留用户填写的准确转写"
    );
    assert!(
        state.zipvoice_reference_importing(),
        "合法 ZipVoice 导入开始后必须进入 importing 状态"
    );
}

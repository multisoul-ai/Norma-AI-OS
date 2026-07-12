use super::*;

/// VS-MODEL-1：托管 Paraformer 已安装时，旧语音设置应把模型 ID 解析为可录音目录。
///
/// 数据构造（含关键数值的推导过程）：
///   app db               = <temp>/app.sqlite
///   managed model root   = <temp>/models
///   selected model id    = paraformer-bilingual-zh-en-int8
///   selected model dir   = <temp>/models/paraformer-bilingual-zh-en-int8
///   unrelated model dir  = <temp>/models/unrelated-model
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在临时 SettingsStore 写入托管模型选择 `voice.asr.model_id`。
///   2. 创建被选中的托管模型目录和一个未选中的干扰目录。
///   3. 通过 VoiceSettingsState::load_from_store 加载真实持久化边界。
///   4. 调用 sherpa_provider，读取语音启动实际采用的模型目录。
///
/// 预期结果：
///   - 正断言：provider 使用被选中的托管 Paraformer 目录，录音启动可继续。
///   - 负断言：provider 不得使用无关模型目录，也不得退回不存在的 bundle 路径。
#[test]
fn test_managed_paraformer_selection_resolves_to_installed_model_directory() {
    let temp = tempfile::tempdir().expect("测试应创建临时 app data 目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let selected_model_dir = temp
        .path()
        .join("models")
        .join("paraformer-bilingual-zh-en-int8");
    let unrelated_model_dir = temp.path().join("models").join("unrelated-model");
    std::fs::create_dir_all(&selected_model_dir).expect("测试应创建已安装的托管模型目录");
    std::fs::create_dir_all(&unrelated_model_dir).expect("测试应创建未选模型干扰目录");
    store
        .set_string("voice.asr.model_id", "paraformer-bilingual-zh-en-int8")
        .expect("测试应保存托管 ASR 模型选择");

    let state = VoiceSettingsState::load_from_store(store);
    let provider = state
        .sherpa_provider()
        .expect("已安装且被选中的托管 Paraformer 应能创建 provider");

    assert_eq!(
        provider.model_dir(),
        selected_model_dir.as_path(),
        "provider 应使用 SettingsStore 同级 models 下被选中的托管模型目录"
    );
    assert_ne!(
        provider.model_dir(),
        unrelated_model_dir.as_path(),
        "provider 不得忽略模型 ID 而选择无关模型目录"
    );
}

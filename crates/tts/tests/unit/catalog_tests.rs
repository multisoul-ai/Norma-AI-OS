use crate::{builtin_model_catalog, TtsBackend, TtsReleaseGate};

/// TTS-CATALOG-1：内置轻量模型必须声明 Matcha 中英双语包的完整安装契约。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog models   = 3（Matcha zh-en + Kokoro multi-language + ZipVoice experimental）
///   archive bytes    = 79,033,838（已下载发布包的 Content-Length）
///   vocoder bytes    = 53,882,848（已下载 16 kHz Vocos 的 Content-Length）
///   download total   = 79,033,838 + 53,882,848 = 132,916,686 bytes
///   sample rate      = 16,000 Hz（模型发布页与实际推理输出一致）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过公共目录 API 查找 `matcha-icefall-zh-en`；
///   2. 读取语言、采样率、归档文件与辅助声码器元数据；
///   3. 检查旧的 `matcha-icefall-zh-baker` 不再暴露给用户选择。
///
/// 预期结果：
///   - 正断言：zh-en 模型及其中英能力、16 kHz Vocos 和 eSpeak 数据契约完整存在；
///   - 负断言：旧中文 Baker 模型和它的 dict 文件契约不得继续出现在产品目录中。
#[test]
fn test_builtin_catalog_declares_matcha_zh_en_installation_contract() {
    let catalog = builtin_model_catalog();
    let matcha = catalog
        .find_by_str("matcha-icefall-zh-en")
        .expect("内置目录应包含 Matcha zh-en 中英双语模型");
    let archive = matcha.package().archive();
    let support = matcha
        .package()
        .support_artifacts()
        .first()
        .expect("Matcha zh-en 必须声明 16 kHz Vocos 辅助文件");

    assert_eq!(
        catalog.entries().len(),
        3,
        "产品目录应包含 Matcha、Kokoro 与实验性 ZipVoice，共 3 项"
    );
    assert_eq!(
        matcha.display_name(),
        "Matcha Chinese-English",
        "Matcha zh-en 应显示为中英双语模型"
    );
    assert_eq!(
        matcha.backend(),
        TtsBackend::Matcha,
        "Matcha zh-en 必须路由到 Matcha 后端"
    );
    assert_eq!(
        matcha.capabilities().languages(),
        &["zh".to_string(), "en".to_string()],
        "Matcha zh-en 必须明确声明中文和英文能力"
    );
    assert_eq!(
        matcha.capabilities().sample_rate_hz(),
        16_000,
        "Matcha zh-en 的输出采样率必须是 16 kHz"
    );
    assert!(matcha.recommended(), "Matcha zh-en 应继续作为推荐轻量模型");
    assert_eq!(
        archive.url(),
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/matcha-icefall-zh-en.tar.bz2",
        "Matcha zh-en 归档必须来自 sherpa-onnx 官方发布地址"
    );
    assert_eq!(
        archive.sha256(),
        "271b804af570400d3bcdcb53bf6e53cc9f75180ee763b9f13eb5eaf2b0d086ef",
        "Matcha zh-en 归档哈希必须匹配已验证下载"
    );
    assert_eq!(
        archive.archive_bytes(),
        79_033_838,
        "Matcha zh-en 归档大小必须匹配已验证下载"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "model-steps-3.onnx"),
        "安装校验必须要求 Matcha ONNX 模型"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "lexicon.txt"),
        "安装校验必须要求 Matcha 词典"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "tokens.txt"),
        "安装校验必须要求 Matcha token 表"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "phone-zh.fst"),
        "安装校验必须要求中文 phone FST"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "date-zh.fst"),
        "安装校验必须要求中文 date FST"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "number-zh.fst"),
        "安装校验必须要求中文 number FST"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "espeak-ng-data/phontab"),
        "安装校验必须要求英文前端所需的 eSpeak 数据"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "espeak-ng-data/phondata"),
        "安装校验必须要求 eSpeak 音素数据"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "espeak-ng-data/phonindex"),
        "安装校验必须要求 eSpeak 音素索引"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "espeak-ng-data/en_dict"),
        "安装校验必须要求 eSpeak 英文词典"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "espeak-ng-data/lang/gmw/en"),
        "安装校验必须要求 eSpeak 英文语言定义"
    );
    assert!(
        !archive
            .required_files()
            .iter()
            .any(|path| path == "dict/stop_words.utf8"),
        "Matcha zh-en 不得沿用 Baker 模型的 dict 文件契约"
    );
    assert_eq!(
        matcha.package().support_artifacts().len(),
        1,
        "Matcha zh-en 应且仅应附带一个 Vocos 辅助文件"
    );
    assert_eq!(
        support.url(),
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos-16khz-univ.onnx",
        "Matcha zh-en 必须使用 16 kHz Vocos 下载地址"
    );
    assert_eq!(
        support.sha256(),
        "b599142a1fb8ff03de3e84ac35ff537c619e56f4267a6fe894851a42844acf9e",
        "16 kHz Vocos 哈希必须匹配已验证下载"
    );
    assert_eq!(
        support.bytes(),
        53_882_848,
        "16 kHz Vocos 大小必须匹配已验证下载"
    );
    assert_eq!(
        support.destination(),
        "vocos-16khz-univ.onnx",
        "16 kHz Vocos 必须安装到模型根目录的固定文件名"
    );
    assert_eq!(
        matcha.package().download_bytes(),
        132_916_686,
        "下载总量必须等于归档与 Vocos 文件大小之和"
    );
    assert!(
        catalog.find_by_str("matcha-icefall-zh-baker").is_none(),
        "旧 Matcha Baker 模型不得继续出现在产品目录中"
    );
}

/// TTS-CATALOG-3：内置目录必须声明 ZipVoice-Distill INT8 的完整实验性安装契约。
///
/// 数据构造（含关键数值的推导过程）：
///   archive bytes    = 109,162,785（GitHub `tts-models` release asset size）
///   vocoder bytes    = 54,157,409（GitHub `vocoder-models` release asset size）
///   download total   = 109,162,785 + 54,157,409 = 163,320,194 bytes
///   sample rate      = 24,000 Hz（官方 ZipVoice Rust 示例使用 24 kHz Vocos）
///   reference inputs = 2（16 kHz mono PCM + exact transcript）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过公共目录 API 查找固定 ZipVoice 模型 ID；
///   2. 读取模型能力、归档文件与 24 kHz Vocos 元数据；
///   3. 检查模型仍为实验性且明确要求参考声音，不会成为默认推荐。
///
/// 预期结果：
///   - 正断言：中英能力、参考声音要求、INT8 encoder/decoder 与 Vocos 契约完整存在；
///   - 负断言：ZipVoice 不得标记为推荐，也不得回退成无需参考声音的固定 Voice 模型。
#[test]
fn test_builtin_catalog_declares_zipvoice_distill_int8_experimental_contract() {
    let catalog = builtin_model_catalog();
    let zipvoice = catalog
        .find_by_str("zipvoice-distill-int8-zh-en-emilia")
        .expect("内置目录应包含实验性 ZipVoice-Distill INT8 模型");
    let archive = zipvoice.package().archive();
    let support = zipvoice
        .package()
        .support_artifacts()
        .first()
        .expect("ZipVoice 必须声明 24 kHz Vocos 辅助文件");

    assert_eq!(
        zipvoice.backend(),
        TtsBackend::ZipVoice,
        "ZipVoice 模型必须路由到 ZipVoice 后端"
    );
    assert_eq!(
        zipvoice.capabilities().languages(),
        &["zh".to_string(), "en".to_string()],
        "ZipVoice 必须明确声明中文和英文能力"
    );
    assert_eq!(
        zipvoice.capabilities().sample_rate_hz(),
        24_000,
        "ZipVoice 输出必须匹配 24 kHz Vocos"
    );
    assert!(
        zipvoice.capabilities().requires_voice_reference(),
        "ZipVoice 必须声明参考音频与准确转写为必填输入"
    );
    assert!(
        zipvoice.experimental(),
        "ZipVoice 在性能与许可 gate 完成前必须保持实验性"
    );
    assert_eq!(
        zipvoice.release_gates(),
        &[
            TtsReleaseGate::M4PerformancePending,
            TtsReleaseGate::EmiliaDistributionAuditPending,
        ],
        "ZipVoice 必须同时保留 M4 性能与 Emilia 分发审计两个显式 release gate"
    );
    assert!(
        !zipvoice.recommended(),
        "ZipVoice 不得取代 Matcha 成为默认推荐模型"
    );
    assert_eq!(
        archive.url(),
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2",
        "ZipVoice 归档必须来自 sherpa-onnx 官方发布地址"
    );
    assert_eq!(
        archive.sha256(),
        "77219c8b40f4ee8d73a7f902305ff6c1128ef9b54461c41b4ca6ed890b6c2803",
        "ZipVoice 归档哈希必须匹配 GitHub release digest"
    );
    assert_eq!(
        archive.archive_bytes(),
        109_162_785,
        "ZipVoice 归档大小必须匹配 GitHub release asset"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "encoder.int8.onnx"),
        "安装校验必须要求 ZipVoice INT8 encoder"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "decoder.int8.onnx"),
        "安装校验必须要求 ZipVoice INT8 decoder"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "tokens.txt"),
        "安装校验必须要求 ZipVoice token 表"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "lexicon.txt"),
        "安装校验必须要求 ZipVoice 词典"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "espeak-ng-data/phontab"),
        "安装校验必须要求 ZipVoice eSpeak 数据"
    );
    assert_eq!(
        support.url(),
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos_24khz.onnx",
        "ZipVoice 必须使用官方 24 kHz Vocos 下载地址"
    );
    assert_eq!(
        support.sha256(),
        "bcb3b970e384161c4d634f0bb9e999ff1c471b34c9bc0b1049a5014065ed3cc0",
        "24 kHz Vocos 哈希必须匹配 GitHub release digest"
    );
    assert_eq!(
        support.bytes(),
        54_157_409,
        "24 kHz Vocos 大小必须匹配 GitHub release asset"
    );
    assert_eq!(
        support.destination(),
        "vocos_24khz.onnx",
        "24 kHz Vocos 必须安装到模型根目录的固定文件名"
    );
    assert_eq!(
        zipvoice.package().download_bytes(),
        163_320_194,
        "下载总量必须等于 ZipVoice 归档与 Vocos 文件大小之和"
    );
    assert_eq!(
        zipvoice.voices().len(),
        1,
        "ZipVoice 应公开一个由参考音频驱动的克隆 Voice 占位身份"
    );
    assert_ne!(
        zipvoice.voices()[0].display_name(),
        "Default Bilingual Voice",
        "ZipVoice 不得伪装成无需参考输入的 Matcha 默认 Voice"
    );
}

/// TTS-CATALOG-2：切换到不包含当前 Voice 的模型时必须回退到该模型推荐 Voice。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog models = 3（中英双语 Matcha + 多 Voice Kokoro + 实验性 ZipVoice）
///   current voice  = "af_heart"（只属于 Kokoro）
///   Matcha voices  = 1（"default"）
///   fallback count = 1（不兼容 Voice → 推荐 Voice）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从内置目录查找 Matcha 模型；
///   2. 请求保留仅属于 Kokoro 的 "af_heart"；
///   3. 通过目录公共 API 解析切换后的有效 Voice。
///
/// 预期结果：
///   - 正断言：结果为 Matcha 的 "default" 推荐 Voice；
///   - 负断言：结果不得保留不存在的 "af_heart"。
#[test]
fn test_model_switch_falls_back_to_recommended_voice() {
    let catalog = builtin_model_catalog();
    let matcha = catalog
        .find_by_str("matcha-icefall-zh-en")
        .expect("内置目录应包含轻量 Matcha 中英双语模型");

    let resolved = matcha.resolve_voice_id(Some("af_heart"));

    assert_eq!(
        resolved.as_str(),
        "default",
        "不属于 Matcha 的 Voice 必须回退到其推荐 default Voice"
    );
    assert_ne!(
        resolved.as_str(),
        "af_heart",
        "切换模型后不得保留新模型不存在的旧 Voice"
    );
}

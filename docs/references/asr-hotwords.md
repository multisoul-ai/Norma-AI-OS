# ASR Hotword 加载开发指南

本文说明 Norma OS 如何为 Sherpa-ONNX 的 Zipformer Bilingual 加载热词，以及开发者如何扩展默认热词或接入运行时配置。

## 1. 当前支持范围

热词仅支持以下模型：

- 模型 ID：`zipformer-bilingual-zh-en-int8-2023-02-20`
- 展示名称：`Zipformer Bilingual`
- 解码方法：`modified_beam_search`
- 建模单元：`cjkchar+bpe`

以下模型收到热词配置时会明确返回错误，不会静默忽略：

- `Paraformer Bilingual`
- `Zipformer Streaming ZH`
- `Zipformer Streaming CTC ZH`

Hotword 不是需要单独下载的“Hotword 2.0 模型”。它是 recognizer 的解码配置；所需的 `bpe.vocab` 已包含在 Zipformer Bilingual 模型归档中，并由模型安装流程校验。

## 2. 默认加载链路

正常录音链路如下：

```text
SherpaProvider::start_recording
  -> session::start
  -> build_online_recognizer_config
  -> AsrHotwordConfig::developer_defaults
  -> build_online_recognizer_config_with_hotwords
  -> sherpa_onnx::OnlineRecognizer::create
```

当选中的模型是 Zipformer Bilingual 时，`build_online_recognizer_config` 会自动加载当前内置热词：

```text
JAVA
KOTLIN
```

默认参数为：

| 参数 | 默认值 | 作用 |
| --- | ---: | --- |
| `hotwords_score` | `2.0` | 提高热词候选的解码得分 |
| `max_active_paths` | `8` | `modified_beam_search` 保留的搜索路径数 |
| `decoding_method` | `modified_beam_search` | 启用支持热词偏置的搜索方式 |
| `modeling_unit` | `cjkchar+bpe` | 支持中英文混合热词 |

相关实现位于：

- `crates/voice_sherpa/src/recognizer_config.rs`
- `crates/voice_sherpa/src/session.rs`
- `crates/voice_sherpa/src/asr_stream.rs`

## 3. 热词文本格式

使用 UTF-8 文本，每行一个热词或短语：

```text
JAVA
SPRING BOOT
RUST
诺玛
语音助手
```

约束：

1. 多词短语必须写在同一行，例如 `SPRING BOOT`。
2. 不要传入只包含空格或换行的文本。
3. 当前封装只支持一个全局 `score`，不支持逐条热词设置不同分数。
4. 文本会按原始 UTF-8 字节传给 Sherpa，不会自动去重、改大小写或做 Unicode 归一化。
5. 新词必须能被模型的 `bpe.vocab`/字符建模单元表示；热词偏置不能让模型识别其词表完全无法表达的内容。

建议在生成文本前先去除空行和重复项，确保输出稳定：

```rust
let text = ["JAVA", "SPRING BOOT", "RUST", "诺玛"].join("\n") + "\n";
```

## 4. 添加内置热词

如果热词由开发团队统一维护，修改 `AsrHotwordConfig::developer_defaults`：

```rust
fn developer_defaults() -> Self {
    Self::new(
        "JAVA\n\
         KOTLIN\n\
         SPRING BOOT\n\
         RUST\n\
         诺玛\n",
        DEFAULT_HOTWORDS_SCORE,
    )
}
```

这种方式会影响所有使用 `build_online_recognizer_config` 的 Zipformer Bilingual 入口，包括录音 session、流式 ASR client 和模型加载验证。修改后需要重新编译应用。

## 5. 构造自定义热词配置

`voice_sherpa` 已公开 `AsrHotwordConfig` 和显式 builder：

```rust
use voice_sherpa::{
    build_online_recognizer_config_with_hotwords,
    AsrHotwordConfig,
};

let hotwords = AsrHotwordConfig::new(
    "JAVA\nSPRING BOOT\nRUST\n诺玛\n",
    2.0,
)
.with_max_active_paths(8);

let recognizer_config = build_online_recognizer_config_with_hotwords(
    &available_model,
    Some(&hotwords),
)?;

let recognizer = sherpa_onnx::OnlineRecognizer::create(&recognizer_config)
    .ok_or_else(|| SpeechError::Recognizer("failed to create OnlineRecognizer".into()))?;
```

传入 `None` 表示不配置热词：

```rust
let recognizer_config =
    build_online_recognizer_config_with_hotwords(&available_model, None)?;
```

注意：显式 builder 只生成 `OnlineRecognizerConfig`。当前 `SherpaProvider::start_recording` 仍调用默认 builder，因此仅在业务层构造自定义配置不会自动改变现有录音 session。

## 6. 接入 Settings 或运行时配置

如果要允许用户不重新编译就修改热词，建议按以下边界接入：

```text
Settings/Storage
  -> 持久化 HotwordSettings
  -> app 后台启动录音
  -> SherpaProvider::start_recording_with_hotwords
  -> session::start(..., Option<AsrHotwordConfig>)
  -> build_online_recognizer_config_with_hotwords
```

建议领域配置：

```rust
pub struct HotwordSettings {
    pub enabled: bool,
    pub phrases: Vec<String>,
    pub score: f32,
    pub max_active_paths: i32,
}
```

转换时统一清洗输入：

```rust
fn to_hotword_config(settings: &HotwordSettings) -> Option<AsrHotwordConfig> {
    if !settings.enabled {
        return None;
    }

    let mut phrases: Vec<&str> = settings
        .phrases
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|phrase| !phrase.is_empty())
        .collect();
    phrases.sort_unstable();
    phrases.dedup();

    if phrases.is_empty() {
        return None;
    }

    let text = phrases.join("\n") + "\n";
    Some(
        AsrHotwordConfig::new(text, settings.score)
            .with_max_active_paths(settings.max_active_paths),
    )
}
```

实现时需要同时覆盖这几个入口，避免行为不一致：

- `SherpaProvider` 的麦克风录音 session。
- `SherpaAsrStreamClient` 的 PCM 流式识别。
- `validate_model_load` 的模型加载检查。
- Settings 持久化与模型切换后的 recognizer 重建。

热词配置在 `OnlineRecognizer` 创建时读取。运行中的 recognizer 不会因为设置值变化自动刷新；修改热词后应结束当前 session，并用新配置创建 recognizer。

## 7. 参数调优

建议先固定一组 WAV 回放集，再调整参数。不要只凭一次口述判断效果。

| 现象 | 调整建议 |
| --- | --- |
| `JAVA` 仍被识别为“抓哇” | 将 score 从 `2.0` 小步提高到 `2.5`，再测试 |
| 普通词被误判成热词 | 降低 score，或删除过短、发音相近的热词 |
| 热词较多时漏召回 | 保持 score 不变，将 paths 从 `8` 调到 `12` 或 `16` |
| 延迟或 CPU 升高 | 减少 paths、缩短热词列表 |

推荐顺序：

1. 固定模型、音频、VAD 和后处理逻辑。
2. 记录无热词基线。
3. 仅调整 `score`，建议每次增加或减少 `0.5`。
4. 再调整 `max_active_paths`。
5. 同时观察热词召回率和非热词误触发率。

代码会拒绝以下配置：

- 空热词文本。
- `score <= 0`、`NaN` 或无穷大。
- `max_active_paths <= 0`。

## 8. 模型安装与升级

Zipformer Bilingual 热词需要以下文件：

```text
encoder-epoch-99-avg-1.int8.onnx
decoder-epoch-99-avg-1.onnx
joiner-epoch-99-avg-1.int8.onnx
tokens.txt
bpe.vocab
```

旧安装可能只有前四个文件。此时安装校验会返回 `UpgradeRequired`，Voice Settings 会显示失败状态。点击 `Retry`，应用会重新下载并安装已固定 SHA-256 的完整模型归档。

不要从其他模型目录复制 `bpe.vocab`：词表必须与当前 Zipformer 模型匹配。

## 9. 验证

修改默认热词或 recognizer 配置后运行：

```bash
cargo test -p voice_sherpa
cargo test -p asr
cargo check -p app
```

如果修改了 Settings、录音入口或模型升级流程，再运行受影响 crate 的聚焦测试，并用真实 App 验收用户可见行为。

相关测试：

- `crates/voice_sherpa/tests/unit/recognizer_config_tests.rs`
- `crates/voice_sherpa/tests/unit/recognizer_config_hotword_tests.rs`
- `crates/asr/tests/unit/installation_tests.rs`

验收至少覆盖：

1. 自定义热词文本精确进入 `hotwords_buf`。
2. `score` 和 `max_active_paths` 没有退回默认值。
3. 解码方式为 `modified_beam_search`。
4. `modeling_unit` 和 `bpe_vocab` 配置正确。
5. 不支持热词的模型明确报错。
6. 缺少 `bpe.vocab` 的旧安装可以通过 Voice Settings 的 `Retry` 恢复。

## 10. 常见问题

### 配置了新热词但录音没有变化

检查实际录音入口是否仍调用 `build_online_recognizer_config`。如果是，它只会读取 `developer_defaults`；运行时热词必须继续传到 `session::start` 并用于创建 recognizer。

### 修改 Settings 后为什么必须重建 recognizer

`hotwords_buf` 是 `OnlineRecognizerConfig` 的创建参数，不是当前 session 的动态输入。已有 recognizer 不会热更新配置。

### 热词越多是否越好

不是。长列表会扩大候选竞争和搜索成本，也可能提高误识别率。优先保留高价值、容易被错识别的领域词。

### score 是否越高越好

不是。过高会把发音相近的普通词强行偏置成热词。必须同时测热词召回和非热词误触发。

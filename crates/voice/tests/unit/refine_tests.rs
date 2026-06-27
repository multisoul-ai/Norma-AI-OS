use super::{refine_prompt, trace_prompt, RefineError, TranscriptRefiner};
use crate::ollama::voice_trace_enabled_from_value;

/// R-1: from_env_or_default 应读取 MULTISOUL_REFINE_MODEL 优先于 MULTISOUL_OLLAMA_MODEL。
///
/// 数据构造（含关键数值的推导过程）：
///   MULTISOUL_REFINE_MODEL = "custom-refine-model"
///   MULTISOUL_OLLAMA_MODEL = "base-model"（应被忽略）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 设置两个 env var
///   2. 调用 from_env_or_default
///   3. 检查 model 字段
///
/// 预期结果：
///   - 正断言：model = "custom-refine-model"
///   - 负断言：model 不应为 "base-model"
#[test]
fn test_refiner_uses_refine_model_env_var_over_ollama_model() {
    unsafe {
        std::env::set_var("MULTISOUL_REFINE_MODEL", "custom-refine-model");
        std::env::set_var("MULTISOUL_OLLAMA_MODEL", "base-model");
    }

    let refiner = TranscriptRefiner::from_env_or_default();

    assert_eq!(
        refiner.model(),
        "custom-refine-model",
        "MULTISOUL_REFINE_MODEL should take priority when set"
    );
    assert_ne!(
        refiner.model(),
        "base-model",
        "MULTISOUL_OLLAMA_MODEL must not override MULTISOUL_REFINE_MODEL"
    );

    unsafe {
        std::env::remove_var("MULTISOUL_REFINE_MODEL");
        std::env::remove_var("MULTISOUL_OLLAMA_MODEL");
    }
}

/// R-2: refine() 在连接拒绝时应返回 RefineError::Timeout（不 panic，不阻塞）。
///
/// 数据构造（含关键数值的推导过程）：
///   endpoint = http://127.0.0.1:19999（无监听端口）
///   timeout = 500ms
///   负向边界 = 测试不依赖真实 Ollama 服务
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造指向无效端口的 refiner
///   2. 调用 refine("test input")
///   3. 检查错误类型
///
/// 预期结果：
///   - 正断言：返回 Err(RefineError::Timeout)
///   - 负断言：不应 panic 或要求本机启动 Ollama
#[test]
fn test_refiner_returns_timeout_on_connection_refused() {
    let refiner = TranscriptRefiner::new(
        "http://127.0.0.1:19999".to_string(),
        "qwen2.5:1.5b".to_string(),
    );

    let result = refiner.refine("test input");

    assert!(
        matches!(result, Err(RefineError::Timeout)),
        "connection refused should map to RefineError::Timeout, got {:?}",
        result
    );
}

/// R-3: refine_prompt 应包含原始 transcript 并禁止扩写。
///
/// 数据构造（含关键数值的推导过程）：
///   raw = "再哪里"
///   prompt 必须原样包含 raw，避免纠错前丢失同音字上下文
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 refine_prompt("再哪里")
///   2. 检查 prompt 字符串
///
/// 预期结果：
///   - 正断言：prompt 包含 "再哪里"
///   - 正断言：prompt 包含 "English" 或 "英文" 相关指令
///   - 正断言：prompt 明确禁止把英文翻译成中文
///   - 负断言：prompt 不应为空
#[test]
fn test_refine_prompt_contains_transcript_and_english_constraint() {
    let prompt = refine_prompt("打开 terminal 输入 hello world");

    assert!(
        prompt.contains("打开 terminal 输入 hello world"),
        "prompt must embed the raw transcript verbatim"
    );
    assert!(!prompt.is_empty(), "prompt must not be empty");
    assert!(
        prompt.to_ascii_lowercase().contains("english") || prompt.contains("英文"),
        "prompt must instruct the model to keep English technical terms"
    );
    assert!(
        prompt.contains("Do NOT translate English") && prompt.contains("Latin-script"),
        "prompt must explicitly forbid translating spoken English into Chinese"
    );
}

/// R-4: refiner 完整 prompt 日志应复用 MULTISOUL_VOICE_TRACE 开关。
///
/// 数据构造（含关键数值的推导过程）：
///   enabled value = "1"，与 planner 现有 trace 开关一致
///   disabled value = "false"，不应输出完整 prompt
///   prompt = refine_prompt("再哪里")，长度 > raw，说明包含完整指令
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 voice_trace_enabled_from_value 解析 "1" 与 "false"
///   2. 构造 refiner prompt
///   3. trace 开启时调用 trace_prompt(prompt, true)
///   4. trace 关闭时调用 trace_prompt(prompt, false)
///
/// 预期结果：
///   - 正断言：trace 开启时返回完整 prompt
///   - 正断言：完整 prompt 包含 raw transcript
///   - 负断言：trace 关闭时不返回 prompt，避免默认日志泄露完整内容
#[test]
fn test_refiner_trace_prompt_uses_voice_trace_gate() {
    let prompt = refine_prompt("再哪里");
    let enabled = voice_trace_enabled_from_value(Some("1".to_string()));
    let disabled = voice_trace_enabled_from_value(Some("false".to_string()));

    assert!(
        enabled,
        "MULTISOUL_VOICE_TRACE=1 应开启完整 refiner prompt 日志"
    );
    assert!(
        !disabled,
        "MULTISOUL_VOICE_TRACE=false 不应开启完整 refiner prompt 日志"
    );
    assert_eq!(
        trace_prompt(&prompt, enabled),
        Some(prompt.as_str()),
        "trace 开启时日志路径应拿到完整 refiner prompt"
    );
    assert!(
        trace_prompt(&prompt, enabled)
            .expect("trace enabled should expose prompt")
            .contains("再哪里"),
        "完整 refiner prompt 应包含原始 transcript"
    );
    assert_eq!(
        trace_prompt(&prompt, disabled),
        None,
        "trace 关闭时不应把完整 prompt 交给日志路径"
    );
}

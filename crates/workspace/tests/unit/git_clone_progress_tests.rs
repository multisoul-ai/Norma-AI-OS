use super::{parse_git_progress_line, CloneStage};

/// GC-P1：Git stderr 的主要 clone 阶段应映射为稳定阶段与百分比。
///
/// 数据构造（含关键数值的推导过程）：
///   connecting(无百分比) + receiving(42%) + resolving(100%) = 3 行 Git 输出。
///   百分比边界：42 位于 0..=100；100 为完成上界。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 `Cloning into` 启动行。
///   2. 解析带吞吐信息的 Receiving objects 行。
///   3. 解析 Resolving deltas 完成行。
///
/// 预期结果：
///   - 正断言：三行分别映射 Connecting、Receiving、Resolving。
///   - 正断言：Receiving=42%，Resolving=100%。
///   - 负断言：Connecting 不得伪造百分比，阶段之间不得混淆。
#[test]
fn git_progress_parser_maps_clone_stages_and_percentages() {
    let connecting = parse_git_progress_line("Cloning into 'codex'...")
        .expect("Git clone 启动行应生成 Connecting 进度");
    let receiving =
        parse_git_progress_line("Receiving objects:  42% (42/100), 1.20 MiB | 2.00 MiB/s")
            .expect("Receiving objects 行应生成 Receiving 进度");
    let resolving = parse_git_progress_line("Resolving deltas: 100% (18/18), done.")
        .expect("Resolving deltas 行应生成 Resolving 进度");

    assert_eq!(
        connecting.stage,
        CloneStage::Connecting,
        "Cloning into 应映射为 Connecting"
    );
    assert_eq!(
        connecting.percent, None,
        "连接阶段没有可信总量，不应伪造百分比"
    );
    assert_eq!(
        receiving.stage,
        CloneStage::Receiving,
        "Receiving objects 应映射为 Receiving"
    );
    assert_eq!(
        receiving.percent,
        Some(42),
        "Receiving objects 的 42% 应被精确提取"
    );
    assert_eq!(
        resolving.stage,
        CloneStage::Resolving,
        "Resolving deltas 应映射为 Resolving"
    );
    assert_eq!(
        resolving.percent,
        Some(100),
        "Resolving deltas 完成行应提取 100%"
    );
    assert_ne!(
        receiving.stage, resolving.stage,
        "接收对象与解析 delta 不得显示为同一阶段"
    );
}

/// GC-P2：控制字符、remote 前缀与未知噪声不应污染用户可见进度。
///
/// 数据构造（含关键数值的推导过程）：
///   carriage-return 前缀(1) + remote Counting(75%)(1) + 未知 warning(1) = 3 行。
///   合法解析数 = 2；忽略数 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 清理 Receiving 行首尾的 `\r`。
///   2. 移除 `remote: ` 前缀并识别 Counting objects。
///   3. 对未知 warning 返回 None。
///
/// 预期结果：
///   - 正断言：Receiving 与 Counting 均被识别并提取百分比。
///   - 负断言：未知 warning 不生成误导性的进度事件。
#[test]
fn git_progress_parser_handles_remote_prefix_and_ignores_unknown_noise() {
    let receiving = parse_git_progress_line("\rReceiving objects:  8% (8/100)\r")
        .expect("带 carriage return 的 Receiving 行仍应被识别");
    let counting = parse_git_progress_line("remote: Counting objects:  75% (3/4)")
        .expect("remote 前缀的 Counting 行应被识别");
    let noise = parse_git_progress_line("warning: redirecting to an unrelated endpoint");

    assert_eq!(
        receiving.percent,
        Some(8),
        "清理 carriage return 后应提取 8%"
    );
    assert_eq!(
        counting.stage,
        CloneStage::Counting,
        "remote Counting objects 应映射为 Counting"
    );
    assert_eq!(counting.percent, Some(75), "Counting objects 应提取 75%");
    assert!(noise.is_none(), "未知 warning 不得变成用户可见的阶段进度");
    assert_ne!(
        counting.stage,
        CloneStage::Receiving,
        "Counting 不得被误报为 Receiving"
    );
}

/// GC-P3：越界或畸形百分比不应进入 UI 状态。
///
/// 数据构造（含关键数值的推导过程）：
///   最大合法百分比 = 100；越界值 = 101 = 100 + 1；畸形值 = "xx"。
///   输入行数 = 2，预期均保留阶段但 percent=None。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 识别两行 Receiving objects 的阶段前缀。
///   2. 尝试解析 `%` 之前的 token。
///   3. 对 >100 或非数字 token 放弃百分比。
///
/// 预期结果：
///   - 正断言：两行仍返回 Receiving 阶段，避免进度文案消失。
///   - 负断言：101 与 xx 均不得成为 percent。
#[test]
fn git_progress_parser_discards_out_of_range_or_malformed_percentages() {
    let out_of_range = parse_git_progress_line("Receiving objects: 101% (101/100)")
        .expect("已识别阶段的越界百分比行仍应返回进度阶段");
    let malformed = parse_git_progress_line("Receiving objects: xx% (8/100)")
        .expect("已识别阶段的畸形百分比行仍应返回进度阶段");

    assert_eq!(
        out_of_range.stage,
        CloneStage::Receiving,
        "越界百分比不应影响 Receiving 阶段识别"
    );
    assert_eq!(out_of_range.percent, None, "101% 超出 0..=100，必须丢弃");
    assert_eq!(
        malformed.stage,
        CloneStage::Receiving,
        "畸形百分比不应影响 Receiving 阶段识别"
    );
    assert_eq!(malformed.percent, None, "非数字百分比不得进入 UI 状态");
    assert_ne!(
        out_of_range.percent,
        Some(100),
        "101% 不得被静默截断为 100%，否则会掩盖解析错误"
    );
}

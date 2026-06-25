use super::*;

/// 左侧菜单列表：临时隐藏期间不应渲染 workspace 左侧纵向菜单。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace_left_rail_visible() = false（临时隐藏开关；后续恢复时只需改回 true）
///   左侧菜单位置 = left 24px, top 178px, width 46px（当前被开关包住，不进入渲染树）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 workspace_left_rail_visible()
///   2. false → render_workspace_chrome 跳过左侧菜单列表分支
///
/// 预期结果：
///   - 正断言：左侧菜单开关为 false，表示列表应被隐藏
///   - 负断言：左侧菜单开关不应为 true，否则用户仍会看到暂时隐藏的菜单列表
#[test]
fn test_workspace_left_rail_is_temporarily_hidden() {
    let left_rail_visible = workspace_left_rail_visible();

    assert!(
        !left_rail_visible,
        "左侧菜单列表临时隐藏期间 workspace_left_rail_visible() 应返回 false"
    );
    assert!(
        !left_rail_visible,
        "左侧菜单列表不应继续标记为可见，否则渲染树仍会挂载该列表"
    );
}

/// 底部语音输入菜单：临时隐藏渲染入口，但不删除 BottomBar 语音功能代码。
///
/// 数据构造（含关键数值的推导过程）：
///   bottom_voice_bar_visible() = false（临时隐藏开关；后续恢复只需改回 true）
///   app_chrome.rs 生产渲染源码 = include_str!("../../src/app/chrome.rs") 在 #[cfg(test)] 前的片段
///   app_voice.rs 源码 = include_str!("../../src/app_voice.rs")（底部入口处理函数应保留）
///   voice_control.rs 源码 = include_str!("../../src/voice_control.rs")（BottomBar trigger 应保留）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 bottom_voice_bar_visible() → false，render_workspace_chrome 不挂载底部语音菜单
///   2. 检查 app_voice.rs 仍保留 on_voice_bar_mouse_down / on_voice_bar_mouse_up
///   3. 检查 voice_control.rs 仍保留 BottomBar trigger
///
/// 预期结果：
///   - 正断言：底部语音菜单开关为 false
///   - 正断言：底部语音处理函数与 BottomBar trigger 仍存在
///   - 负断言：可见性开关不应为 true，否则底部菜单仍会显示
#[test]
fn test_bottom_voice_bar_is_hidden_without_deleting_functionality() {
    let bottom_voice_visible = bottom_voice_bar_visible();
    let app_voice_source = include_str!("../../src/app/voice/mod.rs");
    let voice_control_source = include_str!("../../src/app/voice/control.rs");

    assert!(
        !bottom_voice_visible,
        "底部语音输入菜单应通过 bottom_voice_bar_visible() 默认隐藏"
    );
    assert!(
        !bottom_voice_visible,
        "底部语音输入菜单不应继续标记为可见，否则 Canvas 底部仍会挂载入口"
    );
    assert!(
        app_voice_source.contains("on_voice_bar_mouse_down"),
        "隐藏菜单不应删除底部语音 mouse down 处理函数，后续恢复入口仍可复用"
    );
    assert!(
        app_voice_source.contains("on_voice_bar_mouse_up"),
        "隐藏菜单不应删除底部语音 mouse up 处理函数，后续恢复入口仍可复用"
    );
    assert!(
        voice_control_source.contains("BottomBar"),
        "隐藏菜单不应删除 VoiceTrigger::BottomBar 功能分支"
    );
}

/// 顶部 agent 菜单：截图中的旧缩写按钮应替换为完整名称，并移除末尾两个占位按钮。
///
/// 数据构造（含关键数值的推导过程）：
///   source = include_str!("../../src/app/chrome.rs")（检查当前 chrome 渲染源码）
///   render_source = source 中 #[cfg(test)] 前的生产渲染区，避免测试注释误命中
///   expected_1 = "Claude" + " Code"（CL 应展开为完整产品名）
///   expected_2 = "Co" + "dex"（CX 应展开为完整产品名）
///   forbidden_1 = "\"C" + "L\""（截图里的旧 Claude 缩写 literal）
///   forbidden_2 = "\"C" + "X\""（截图里的旧 Codex 缩写 literal）
///   forbidden_3 = "toolbar" + "-reflow"（截图里 CX 后面的 [] 按钮 id）
///   forbidden_4 = "chrome_button(theme, \"*\", false)"（截图里最后一个星号占位按钮）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app_chrome.rs 的生产渲染源码
///   2. 确认 agent 按钮包含新的完整文案
///   3. 搜索旧缩写 literal 与截图里的两个末尾无用按钮
///
/// 预期结果：
///   - 正断言：Claude agent 顶部按钮显示为 Claude Code
///   - 正断言：Codex agent 顶部按钮显示为 Codex
///   - 负断言：CL/CX 旧缩写不应继续出现在生产渲染源码中
///   - 负断言：[] reflow 按钮和 * 占位按钮不应继续挂载
#[test]
fn test_top_toolbar_agent_labels_are_expanded_and_trailing_buttons_removed() {
    let source = include_str!("../../src/app/chrome.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let claude_label = format!("{}{}", "Claude", " Code");
    let codex_label = format!("{}{}", "Co", "dex");
    let legacy_claude_literal = format!("\"{}{}\"", "C", "L");
    let legacy_codex_literal = format!("\"{}{}\"", "C", "X");
    let trailing_reflow_id = format!("{}{}", "toolbar", "-reflow");
    let trailing_star_button = format!("{}{}", "chrome_button(theme, \"", "*\", false)");

    assert!(
        render_source.contains(&claude_label),
        "顶部 Claude agent 按钮应显示 Claude Code，不能继续只显示 CL"
    );
    assert!(
        render_source.contains(&codex_label),
        "顶部 Codex agent 按钮应显示 Codex，不能继续只显示 CX"
    );
    assert!(
        !render_source.contains(&legacy_claude_literal),
        "顶部 Claude agent 按钮不应继续渲染 CL 缩写 literal"
    );
    assert!(
        !render_source.contains(&legacy_codex_literal),
        "顶部 Codex agent 按钮不应继续渲染 CX 缩写 literal"
    );
    assert!(
        !render_source.contains(&trailing_reflow_id),
        "顶部菜单不应继续挂载截图中 CX 后面的 [] reflow 按钮"
    );
    assert!(
        !render_source.contains(&trailing_star_button),
        "顶部菜单不应继续挂载截图中最后的星号占位按钮"
    );
}

/// 右上角反馈入口：workspace chrome 不应再挂载反馈按钮。
///
/// 数据构造（含关键数值的推导过程）：
///   source = include_str!("../../src/app/chrome.rs")（检查当前 chrome 源码）
///   render_source = source 中 #[cfg(test)] 前的生产渲染区，避免测试注释误命中
///   toolbar id = "toolbar" + "-new-terminal"（应继续存在，证明顶部工具栏没有被误删）
///   forbidden label = "Feed" + "back"（用户要求删除的按钮文案）
///   forbidden id = "workspace-" + "feedback"（旧按钮命中 id）
///   forbidden handler = "on_" + "feedback" + "_click"（旧按钮点击处理入口）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app_chrome.rs 的生产渲染源码
///   2. 确认 toolbar-new-terminal 仍存在 → chrome 基本结构仍在
///   3. 搜索旧反馈按钮的文案、id、点击 handler
///
/// 预期结果：
///   - 正断言：顶部新终端按钮仍存在
///   - 负断言：旧反馈文案、id、handler 不应继续出现在 chrome 渲染源码中
#[test]
fn test_workspace_chrome_omits_feedback_entry() {
    let source = include_str!("../../src/app/chrome.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let toolbar_new_terminal_id = concat!("toolbar", "-new-terminal");
    let feedback_label = format!("{}{}", "Feed", "back");
    let feedback_id = format!("{}{}", "workspace-", "feedback");
    let feedback_handler = format!("{}{}{}", "on_", "feedback", "_click");

    assert!(
        render_source.contains(toolbar_new_terminal_id),
        "workspace chrome 应保留顶部新终端按钮，避免测试因 chrome 被整体删除而误通过"
    );
    assert!(
        !render_source.contains(&feedback_label),
        "workspace chrome 不应继续渲染右上角反馈按钮文案"
    );
    assert!(
        !render_source.contains(&feedback_id),
        "workspace chrome 不应继续挂载旧反馈按钮 id"
    );
    assert!(
        !render_source.contains(&feedback_handler),
        "workspace chrome 不应继续绑定旧反馈按钮点击 handler"
    );
}

/// 顶部设置入口：Canvas 顶部菜单应挂载齿轮设置按钮，且不移除现有建卡入口。
///
/// 数据构造（含关键数值的推导过程）：
///   source = include_str!("../../src/app/chrome.rs")（检查当前 chrome 渲染源码）
///   render_source = source 中 #[cfg(test)] 前的生产渲染区，避免测试注释误命中
///   expected id = "toolbar" + "-settings"
///   expected icon = "⚙"（用户要求使用设置按钮 icon）
///   existing ids/text = toolbar-new-terminal、Claude Code、Codex
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app_chrome.rs 的生产渲染源码
///   2. 确认 toolbar-settings 与齿轮 icon 出现在顶部工具栏
///   3. 确认现有新终端、Claude Code、Codex 入口仍存在
///
/// 预期结果：
///   - 正断言：设置按钮 id 与齿轮 icon 存在
///   - 正断言：现有三个入口仍存在
///   - 负断言：设置入口不应缺失，否则用户无法打开设置弹窗
#[test]
fn test_top_toolbar_includes_settings_icon_entry() {
    let source = include_str!("../../src/app/chrome.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let settings_id = format!("{}{}", "toolbar", "-settings");
    let settings_icon = "⚙";
    let terminal_id = concat!("toolbar", "-new-terminal");
    let claude_label = format!("{}{}", "Claude", " Code");
    let codex_label = format!("{}{}", "Co", "dex");

    assert!(
        render_source.contains(&settings_id),
        "顶部菜单应包含 toolbar-settings 设置按钮 id"
    );
    assert!(
        render_source.contains(settings_icon),
        "顶部菜单设置按钮应使用齿轮 icon"
    );
    assert!(
        render_source.contains(terminal_id),
        "新增设置入口不应移除顶部新终端按钮"
    );
    assert!(
        render_source.contains(&claude_label),
        "新增设置入口不应移除 Claude Code 按钮"
    );
    assert!(
        render_source.contains(&codex_label),
        "新增设置入口不应移除 Codex 按钮"
    );
    assert!(
        render_source.contains(&settings_id) && render_source.contains(settings_icon),
        "设置入口必须同时具备可定位 id 与可见齿轮 icon"
    );
}

/// 顶部画布滚动入口：Canvas 顶部菜单应挂载箭头 toggle，并保留现有建卡与设置入口。
///
/// 数据构造（含关键数值的推导过程）：
///   source = include_str!("../../src/app/chrome.rs")（检查当前 chrome 渲染源码）
///   render_source = source 中 #[cfg(test)] 前的生产渲染区，避免测试注释误命中
///   expected id = "toolbar" + "-canvas-scroll"
///   expected icon = "↕"（箭头表示画布可滚动）
///   expected handler = "on_" + "toolbar" + "_canvas_scroll"
///   existing ids/text = toolbar-new-terminal、toolbar-settings、Preview
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app_chrome.rs 的生产渲染源码
///   2. 确认画布滚动按钮 id、箭头 icon、点击 handler 都存在
///   3. 确认现有新终端、Preview、设置入口仍存在
///
/// 预期结果：
///   - 正断言：顶部菜单存在 toolbar-canvas-scroll、箭头 icon、toggle handler
///   - 正断言：现有入口仍存在
///   - 负断言：旧的无 id 箭头占位按钮不应继续使用固定 false 状态
#[test]
fn test_top_toolbar_includes_canvas_scroll_arrow_toggle() {
    let source = include_str!("../../src/app/chrome.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let scroll_id = format!("{}{}", "toolbar", "-canvas-scroll");
    let scroll_handler = format!("{}{}{}", "on_", "toolbar", "_canvas_scroll");
    let terminal_id = concat!("toolbar", "-new-terminal");
    let settings_id = concat!("toolbar", "-settings");
    let preview_label = "Preview";
    let legacy_static_button = "chrome_button(theme, \"v\", false)";

    assert!(
        render_source.contains(&scroll_id),
        "顶部菜单应包含 toolbar-canvas-scroll 画布滚动 toggle"
    );
    assert!(
        render_source.contains(TOOLBAR_CANVAS_SCROLL_ICON),
        "画布滚动 toggle 应使用箭头 icon 表示可滚动"
    );
    assert!(
        render_source.contains(&scroll_handler),
        "画布滚动 toggle 应绑定 on_toolbar_canvas_scroll 点击 handler"
    );
    assert!(
        render_source.contains(terminal_id),
        "新增画布滚动 toggle 不应移除顶部新终端按钮"
    );
    assert!(
        render_source.contains(settings_id),
        "新增画布滚动 toggle 不应移除顶部设置按钮"
    );
    assert!(
        render_source.contains(preview_label),
        "新增画布滚动 toggle 不应移除顶部 Preview 按钮"
    );
    assert!(
        !render_source.contains(legacy_static_button),
        "旧的静态 v 占位按钮不应继续存在，否则无法反映画布滚动开关状态"
    );
}

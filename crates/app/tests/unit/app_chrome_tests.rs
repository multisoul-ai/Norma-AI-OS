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

/// 底部语音状态条：保持可见并提示快捷键，但不得成为第二个语音激活入口。
///
/// 数据构造（含关键数值的推导过程）：
///   bottom_voice_bar_visible() = true（voice_sherpa 接入后恢复显示）
///   app_voice.rs 源码 = include_str!("../../src/app/voice/mod.rs")
///   voice_bar.rs 源码 = include_str!("../../src/app/voice/bar.rs")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 bottom_voice_bar_visible() → true，render_workspace_chrome 挂载底部语音菜单
///   2. 检查 voice_bar.rs 显示 ⌘E 键帽
///   3. 检查 app_voice.rs 不再注册底栏 mouse down/up 激活处理
///
/// 预期结果：
///   - 正断言：底部语音菜单开关为 true（voice_sherpa 接入后已启用）
///   - 正断言：底栏保持状态反馈与快捷键提示
///   - 负断言：底栏不得保留第二套鼠标激活入口
#[test]
fn test_bottom_voice_bar_is_visible_with_sherpa_provider() {
    let bottom_voice_visible = bottom_voice_bar_visible();
    let app_voice_source = include_str!("../../src/app/voice/mod.rs");
    let voice_bar_source = include_str!("../../src/app/voice/bar.rs");
    assert!(
        bottom_voice_visible,
        "voice_sherpa 接入后底部语音输入菜单应通过 bottom_voice_bar_visible() 开启"
    );
    assert!(
        voice_bar_source.contains("IDLE_KEYCAPS"),
        "底部状态条应继续提示唯一的 ⌘E 激活快捷键"
    );
    assert!(
        !app_voice_source.contains("on_voice_bar_mouse_down"),
        "底部状态条不得保留 mouse down 语音激活入口"
    );
    assert!(
        !app_voice_source.contains("on_voice_bar_mouse_up"),
        "底部状态条不得保留 mouse up 听写提交入口"
    );
}

/// Product UI Dock：Codex 保留直达入口，其余 Agent 入口收纳到 New 菜单且能力不丢失。
///
/// 数据构造（含关键数值的推导过程）：
///   chrome source = Command Dock 生产渲染源码
///   menu source = New 菜单生产渲染源码
///   Dock 直达 Agent = Codex 1 个；菜单 Agent = Claude/Codex/Cursor/OpenCode 4 个
///   forbidden_1 = "\"C" + "L\""（截图里的旧 Claude 缩写 literal）
///   forbidden_2 = "\"C" + "X\""（截图里的旧 Codex 缩写 literal）
///   forbidden_3 = "chrome_button(theme, \"Claude" + " Code\""（第三步前的文本按钮调用，现已被图标取代）
///   forbidden_4 = "chrome_button(theme, \"Co" + "dex\""（第三步前的文本按钮调用，现已被图标取代）
///   forbidden_5 = "toolbar" + "-reflow"（截图里 CX 后面的 [] 按钮 id）
///   forbidden_6 = "chrome_button(theme, \"" + "*\", false)"（截图里最后一个星号占位按钮）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app_chrome.rs 的生产渲染源码
///   2. 确认 Codex 在 Dock 直达，Claude/Cursor/OpenCode 仍在 New 菜单
///   3. 搜索旧缩写 literal、旧完整文本按钮与无用占位按钮
///
/// 预期结果：
///   - 正断言：Codex 在 Dock，Claude/Cursor/OpenCode 在 New 菜单
///   - 负断言：CL/CX 旧缩写不应继续出现在生产渲染源码中
///   - 负断言：旧完整文本标签 "Claude Code" / "Codex" 不应继续作为 chrome_button 文案
///   - 负断言：[] reflow 按钮和 * 占位按钮不应继续挂载
#[test]
fn test_top_toolbar_agent_entries_use_icon_variants_not_text_labels() {
    let source = include_str!("../../src/app/chrome/dock.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let new_menu_source = include_str!("../../src/app/new_menu.rs");
    let claude_icon_variant = "ToolbarIcon::Claude";
    let codex_icon_variant = "ToolbarIcon::Codex";
    let opencode_icon_variant = "ToolbarIcon::OpenCode";
    let legacy_claude_literal = format!("\"{}{}\"", "C", "L");
    let legacy_codex_literal = format!("\"{}{}\"", "C", "X");
    let legacy_claude_label = format!("{}{}", "chrome_button(theme, \"Claude", " Code\"");
    let legacy_codex_label = format!("{}{}", "chrome_button(theme, \"Co", "dex\"");
    let trailing_reflow_id = format!("{}{}", "toolbar", "-reflow");
    let trailing_star_button = format!("{}{}", "chrome_button(theme, \"", "*\", false)");

    assert!(
        new_menu_source.contains(claude_icon_variant),
        "Claude agent 入口应保留在 New 菜单"
    );
    assert!(
        render_source.contains(codex_icon_variant),
        "顶部 Codex agent 入口应由 chrome_icon_button + ToolbarIcon::Codex 渲染"
    );
    assert!(
        new_menu_source.contains(opencode_icon_variant),
        "OpenCode agent 入口应保留在 New 菜单"
    );
    assert!(
        new_menu_source.contains("ToolbarIcon::Cursor"),
        "Cursor agent 入口应保留在 New 菜单"
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
        !render_source.contains(&legacy_claude_label),
        "第三步重设计后顶部不应继续以 chrome_button(theme, \"Claude Code\", ...) 文本标签形式挂载"
    );
    assert!(
        !render_source.contains(&legacy_codex_label),
        "第三步重设计后顶部不应继续以 chrome_button(theme, \"Codex\", ...) 文本标签形式挂载"
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
///   source = include_str!("../../src/app/chrome/dock.rs")（检查 Command Dock 源码）
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
    let source = include_str!("../../src/app/chrome/dock.rs");
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

/// 底部 Dock 设置入口：齿轮由 SVG 图标 ToolbarIcon::Settings 渲染，旧 ⚙ glyph 不再使用。
///
/// 数据构造（含关键数值的推导过程）：
///   source = include_str!("../../src/app/chrome/dock.rs")（检查 Command Dock 渲染源码）
///   render_source = source 中 #[cfg(test)] 前的生产渲染区，避免测试注释误命中
///   expected id        = "toolbar" + "-settings"
///   expected icon form = "ToolbarIcon::Settings"（SVG 图标变体）
///   existing icons     = ToolbarIcon::Plus/Terminal/Codex/Browser/Preview
///   forbidden glyph    = "\"⚙\""（第三步前使用的 Unicode 字符）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app_chrome.rs 的生产渲染源码
///   2. 确认 toolbar-settings 与 ToolbarIcon::Settings 出现在底部 Dock
///   3. 确认其它核心入口仍以图标形式存在
///   4. 搜索旧 ⚙ glyph literal，确认已被移除
///
/// 预期结果：
///   - 正断言：设置按钮 id 与 Settings 图标变体同时存在
///   - 正断言：现有三个入口的图标变体仍存在
///   - 负断言：旧 ⚙ glyph literal 不应继续出现
#[test]
fn test_top_toolbar_includes_settings_icon_entry() {
    let source = include_str!("../../src/app/chrome/dock.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let settings_id = concat!("toolbar", "-settings");
    let settings_icon_variant = "ToolbarIcon::Settings";
    let plus_icon_variant = "ToolbarIcon::Plus";
    let terminal_icon_variant = "ToolbarIcon::Terminal";
    let codex_icon_variant = "ToolbarIcon::Codex";
    let browser_icon_variant = "ToolbarIcon::Browser";
    let preview_icon_variant = "ToolbarIcon::Preview";
    let legacy_settings_glyph = "\"⚙\"";

    assert!(
        render_source.contains(settings_id),
        "顶部菜单应包含 toolbar-settings 设置按钮 id"
    );
    assert!(
        render_source.contains(settings_icon_variant),
        "顶部菜单设置按钮应使用 ToolbarIcon::Settings 图标变体"
    );
    assert!(
        render_source.contains(plus_icon_variant),
        "新增设置入口不应移除顶部新终端按钮（ToolbarIcon::Plus）"
    );
    assert!(
        render_source.contains(terminal_icon_variant),
        "Settings 入口不应移除 Terminal 直达按钮"
    );
    assert!(
        render_source.contains(codex_icon_variant),
        "新增设置入口不应移除 Codex 图标按钮"
    );
    assert!(
        render_source.contains(browser_icon_variant),
        "Settings 入口不应移除 Browser 直达按钮"
    );
    assert!(
        render_source.contains(preview_icon_variant),
        "Settings 入口不应移除 Preview 直达按钮"
    );
    assert!(
        !render_source.contains(legacy_settings_glyph),
        "第三步重设计后顶部不应继续使用 ⚙ Unicode glyph literal"
    );
    assert!(
        render_source.contains(settings_id) && render_source.contains(settings_icon_variant),
        "设置入口必须同时具备可定位 id 与 Settings 图标变体"
    );
}

/// 顶部画布滚动入口（第三步重设计后）：滚动开关由 ToolbarIcon::Scroll 图标渲染，旧 ↕ glyph 不再使用。
///
/// 数据构造（含关键数值的推导过程）：
///   source = include_str!("../../src/app/chrome/dock.rs")（检查 Command Dock 渲染源码）
///   render_source = source 中 #[cfg(test)] 前的生产渲染区，避免测试注释误命中
///   expected id        = "toolbar" + "-canvas-scroll"
///   expected icon form = "ToolbarIcon::Scroll"（第三步起的 SVG 图标变体）
///   expected handler   = "on_" + "toolbar" + "_canvas_scroll"
///   existing ids       = toolbar-new-terminal、toolbar-settings
///   forbidden glyph    = "\"↕\""（第三步前的 Unicode 箭头 glyph）
///   forbidden button   = "chrome_button(theme, \"v\", false)"（更早的静态占位按钮）
///   forbidden const    = "TOOLBAR_CANVAS_SCROLL_ICON"（第三步删除的常量）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app_chrome.rs 的生产渲染源码
///   2. 确认画布滚动按钮 id、Scroll 图标变体、点击 handler 都存在
///   3. 确认现有新终端、设置入口仍存在
///   4. 搜索旧 ↕ glyph literal、旧静态 v 占位按钮、已删除的 TOOLBAR_CANVAS_SCROLL_ICON 常量
///
/// 预期结果：
///   - 正断言：顶部菜单存在 toolbar-canvas-scroll、Scroll 图标变体、toggle handler
///   - 正断言：现有新终端与设置入口仍存在
///   - 负断言：旧 ↕ glyph literal、旧静态 v 占位按钮、TOOLBAR_CANVAS_SCROLL_ICON 常量不应继续存在
#[test]
fn test_top_toolbar_includes_canvas_scroll_icon_toggle() {
    let source = include_str!("../../src/app/chrome/dock.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let scroll_id = concat!("toolbar", "-canvas-scroll");
    let scroll_handler = format!("{}{}{}", "on_", "toolbar", "_canvas_scroll");
    let scroll_icon_variant = "ToolbarIcon::Scroll";
    let terminal_icon_variant = "ToolbarIcon::Plus";
    let settings_id = concat!("toolbar", "-settings");
    let legacy_arrow_glyph = "\"↕\"";
    let legacy_static_button = "chrome_button(theme, \"v\", false)";
    let legacy_scroll_const = "TOOLBAR_CANVAS_SCROLL_ICON";

    assert!(
        render_source.contains(scroll_id),
        "顶部菜单应包含 toolbar-canvas-scroll 画布滚动 toggle"
    );
    assert!(
        render_source.contains(scroll_icon_variant),
        "画布滚动 toggle 应使用 ToolbarIcon::Scroll 图标变体表示可滚动"
    );
    assert!(
        render_source.contains(&scroll_handler),
        "画布滚动 toggle 应绑定 on_toolbar_canvas_scroll 点击 handler"
    );
    assert!(
        render_source.contains(terminal_icon_variant),
        "新增画布滚动 toggle 不应移除顶部新终端按钮"
    );
    assert!(
        render_source.contains(settings_id),
        "新增画布滚动 toggle 不应移除顶部设置按钮"
    );
    assert!(
        !render_source.contains(legacy_arrow_glyph),
        "第三步重设计后顶部不应继续使用 ↕ Unicode glyph literal"
    );
    assert!(
        !render_source.contains(legacy_static_button),
        "旧的静态 v 占位按钮不应继续存在，否则无法反映画布滚动开关状态"
    );
    assert!(
        !render_source.contains(legacy_scroll_const),
        "第三步删除的 TOOLBAR_CANVAS_SCROLL_ICON 常量不应再出现"
    );
}

/// 壁纸 img 的 id 必须随渲染源派生（render_id_key），而非固定字符串。
///
/// 回归根因：GPUI 0.2.2 按 element-id 跨渲染保留 `ImgState.frame_index`。多帧 GIF 可把
/// frame_index 推到 >1；若随后切到单帧静态图且复用同一 id，旧 frame_index 索引新单帧
/// RenderImage.data → `index out of bounds` panic（assets.rs:79）。源派生 id 让切换即换 id，
/// GPUI 丢弃旧 state、frame_index 归零；同源稳定以维持 GIF 多帧动画。
///
/// 数据构造：source = chrome.rs 生产区（#[cfg(test)] 前）；
///   render_id_key 锚点（必含）；固定 .id("wallpaper") 反锚点（必不含）；img( 锚点（必含）。
///
/// 预期结果：含 img(、含 render_id_key、不含 .id("wallpaper")（裸固定 id 会触发越界 panic）。
#[test]
fn test_wallpaper_img_has_source_derived_id() {
    let source = include_str!("../../src/app/chrome.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);

    assert!(render_source.contains("img("), "壁纸图层应以 img 元素渲染");
    assert!(
        render_source.contains("render_id_key"),
        "壁纸 img 的 id 必须由 render_id_key 派生，否则切源时 GPUI 旧 frame_index 越界 panic"
    );
    assert!(
        !render_source.contains(".id(\"wallpaper\")"),
        "壁纸 img 不应再用固定 .id(\"wallpaper\")；跨源复用 ImgState 会导致 index-out-of-bounds panic"
    );
}

/// 视频壁纸 live 路径必须使用 420f CVPixelBuffer + surface。
///
/// 数据构造：
///   render_src = chrome.rs 生产渲染区；video_src = video_wallpaper.rs live 壁纸生产源码
///   正锚点：WallpaperKind::Video、render_video_wallpaper_surface、surface(、background_executor、spawn
///   反锚点：render_video_wallpaper_image、RenderImage、render_image_from_rgba、next_frame_rgba
///
/// 执行过程：
///   1. 读取 chrome.rs，确认 Video 分支存在且调用 surface 渲染入口
///   2. 读取 video_wallpaper.rs，确认 live 帧后台解码并通过 surface(CVPixelBuffer) 渲染
///
/// 预期结果：
///   - 正断言：Video 分支调用 render_video_wallpaper_surface，后台解码，且使用 surface(
///   - 负断言：live 视频壁纸不应通过 RenderImage / RGBA / img(frame) 路径每帧上屏
#[test]
fn test_video_wallpaper_branch_uses_surface_not_render_images() {
    let chrome_src = include_str!("../../src/app/chrome.rs");
    let render_src = &chrome_src[..chrome_src.find("#[cfg(test)]").unwrap_or(chrome_src.len())];
    let video_src = include_str!("../../src/app/video_wallpaper.rs");

    assert!(
        render_src.contains("WallpaperKind::Video")
            && render_src.contains("render_video_wallpaper_surface"),
        "Video 分支必须调用 render_video_wallpaper_surface，避免回到每帧 RenderImage/img 路径"
    );
    assert!(
        video_src.contains("surface("),
        "video_wallpaper.rs live 视频帧必须通过 gpui::surface(CVPixelBuffer) 渲染"
    );
    assert!(
        video_src.contains("background_executor()") && video_src.contains(".spawn(async move"),
        "video_wallpaper.rs 必须把 AVFoundation open/decode 放到 background_executor worker"
    );
    assert!(
        !video_src.contains("render_video_wallpaper_image"),
        "live 视频壁纸不应再暴露 render_video_wallpaper_image，否则可能继续走 img(RenderImage)"
    );
    assert!(
        !video_src.contains("RenderImage"),
        "live 视频壁纸不应持有 RenderImage；缩略图可用 RenderImage，但 live 每帧不可用"
    );
    assert!(
        !video_src.contains("render_image_from_rgba"),
        "live 视频壁纸不应每帧 RGBA→BGRA 转换后构造 RenderImage"
    );
    assert!(
        !video_src.contains("next_frame_rgba"),
        "live 视频壁纸不应调用 next_frame_rgba；该接口只适合首帧缩略图/离线图片路径"
    );
}

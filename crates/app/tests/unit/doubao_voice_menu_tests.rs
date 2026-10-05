use super::*;
use crate::assets::EmbeddedAssets;
use gpui::AssetSource;
use tts::providers::doubao::find_voice;

/// 音色菜单的展开箭头与已选勾必须随二进制嵌入，不能在真实 App 中消失。
///
/// 数据构造（含关键数值的推导过程）：
///   图标总数 = 收起箭头 1 + 展开箭头 1 + 已选勾 1 = 3；未知路径为负样本。
///   每个图标加载结果必须有非空字节，没有预算或超时参数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 EmbeddedAssets 分别加载组件使用的三个精确图标路径。
///   2. 枚举 icons 目录 → 三个图标都可被发现。
///   3. 加载不存在的音色图标 → 返回 None，不能误用其他资源。
///
/// 预期结果：上下箭头与勾都有非空字节并可枚举；不存在的资源不加载。
#[test]
fn doubao_menu_icons_are_embedded() {
    let assets = EmbeddedAssets;
    let down = assets
        .load("icons/chevron-down.svg")
        .expect("收起箭头加载不应报错");
    let up = assets
        .load("icons/chevron-up.svg")
        .expect("展开箭头加载不应报错");
    let check = assets.load("icons/check.svg").expect("已选勾加载不应报错");
    assert!(
        down.is_some_and(|bytes| !bytes.is_empty()),
        "缺少收起箭头会使选择器看起来不能展开"
    );
    assert!(
        up.is_some_and(|bytes| !bytes.is_empty()),
        "展开菜单需要可见的上箭头"
    );
    assert!(
        check.is_some_and(|bytes| !bytes.is_empty()),
        "缺少勾会丢失已选状态的视觉证据"
    );
    let icons = assets.list("icons").expect("图标目录枚举不应报错");
    assert!(
        icons.iter().any(|name| name.as_ref() == "chevron-down.svg"),
        "收起箭头必须被枚举"
    );
    assert!(
        icons.iter().any(|name| name.as_ref() == "chevron-up.svg"),
        "展开箭头必须被枚举"
    );
    assert!(
        icons.iter().any(|name| name.as_ref() == "check.svg"),
        "已选勾必须被枚举"
    );
    assert!(
        assets
            .load("icons/doubao-voice-missing.svg")
            .expect("未知图标查找不应报错")
            .is_none(),
        "未知资源不能冒充现有音色图标"
    );
}

/// 搜索后的菜单行必须指向匹配音色，搜索本身不能修改已保存的选择。
///
/// 数据构造（含关键数值的推导过程）：
///   官方 Vivi 为已选音色；查询带空格的大写 XIAOHE 应唯一匹配小何。
///   行号 0 = 过滤结果的第一行，行号 1 = 唯一结果之后的越界位置。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立完整目录并搜索 XIAOHE → 过滤为小何。
///   2. 读取确认操作将使用的第一行 → 必须得到小何而非原目录的 Vivi。
///   3. 检查已选值 → 仍为 Vivi，只有确认操作可以改变选择。
///
/// 预期结果：第一行是小何，第二行不存在，已选 Vivi 保留且不是小何。
#[test]
fn doubao_menu_search_maps_filtered_row_without_changing_selection() {
    let selected = find_voice("zh_female_vv_uranus_bigtts").expect("官方目录应包含 Vivi");
    let mut catalog = VoiceCatalog::new(Theme::default(), selected);
    catalog.search("  XIAOHE  ");
    assert_eq!(
        catalog.voice_at(0).map(|voice| voice.id.as_str()),
        Some("zh_female_xiaohe_uranus_bigtts"),
        "过滤后的第一行必须确认小何，不能用原目录索引"
    );
    assert!(
        catalog.voice_at(1).is_none(),
        "唯一匹配之后不能存在额外音色"
    );
    assert_eq!(
        catalog.selected.id, "zh_female_vv_uranus_bigtts",
        "搜索或键盘浏览不能覆盖已选 Vivi"
    );
    assert_ne!(
        catalog.selected.id, "zh_female_xiaohe_uranus_bigtts",
        "小何必须经确认才成为已选音色"
    );
}

/// 外语查询、空结果和清空查询保持目录与当前选择一致。
///
/// 数据构造（含关键数值的推导过程）：
///   查询“阿拉伯”命中 Dina；不存在的 ID 不命中；空查询恢复 294 + 137 = 431 个音色。
///   首行索引 0 用于确认空目录不会返回旧结果，没有预算或超时参数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 搜索语言 → 外语 Dina 可被发现。
///   2. 搜索未知 ID → 空结果不可确认。
///   3. 清空查询 → 完整目录恢复，已选 Vivi 没有变化。
///
/// 预期结果：Dina 存在而 Vivi 不混入语言结果；未知查询为空；清空恢复完整目录。
#[test]
fn doubao_menu_empty_results_and_clear_restore_catalog() {
    let selected = find_voice("zh_female_vv_uranus_bigtts").expect("官方目录应包含 Vivi");
    let mut catalog = VoiceCatalog::new(Theme::default(), selected);
    catalog.search("阿拉伯");
    assert!(
        catalog
            .matching
            .iter()
            .any(|voice| voice.id == "ar_female_dina_uranus_bigtts"),
        "语言搜索必须包含 Dina"
    );
    assert!(
        !catalog.matching.iter().any(|voice| voice.id == selected.id),
        "阿拉伯语搜索不能混入 Vivi"
    );
    catalog.search("no-such-system-voice");
    assert!(catalog.voice_at(0).is_none(), "空目录不能确认残留的旧音色");
    catalog.search("");
    assert_eq!(
        catalog.matching.len(),
        431,
        "清空必须恢复两个官方表格共 431 个音色"
    );
    assert!(
        catalog.matching.iter().any(|voice| voice.id == selected.id),
        "清空后已选 Vivi 必须可见"
    );
    assert_eq!(catalog.selected.id, selected.id, "查询变化不能修改当前音色");
}

/// Tab 应关闭音色菜单，搜索输入的缩进行为不能吞掉关闭操作。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离数据库；已选 Vivi，目录包含 294 + 137 = 431 项。
///   先按 Down 浏览小何，再按 Tab；共 1 次浏览 + 1 次退出，没有确认操作。
///   窗口 1280×800px 是桌面基准，菜单直接挂入 Root，避免依赖设置页滚动位置。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开菜单，绘制并聚焦组件的真实搜索输入。
///   2. Down 改变浏览目标，不能保存小何。
///   3. Tab 必须在 Input 的缩进键绑定前关闭菜单并返回触发器焦点。
///
/// 预期结果：菜单从打开变为关闭；已选 Vivi 保留，小何没有成为已选音色。
#[test]
fn doubao_menu_tab_closes_before_search_input_consumes_key() {
    assert_doubao_menu_tab_cancels("tab");
}

/// Shift-Tab 应取消音色菜单并返回字段焦点，不能保存键盘浏览中的音色。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离数据库与 1280×800px 窗口；默认 Vivi，目录 294 + 137 = 431 项。
///   Down 一次浏览小何，Shift-Tab 一次退出，共 2 次按键、0 次确认。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开菜单并聚焦真实搜索 Input。
///   2. Down 浏览小何，已保存值仍为 Vivi。
///   3. Shift-Tab 捕获 Input 的反缩进动作，关闭菜单并聚焦触发器。
///
/// 预期结果：菜单关闭，焦点返回字段；Vivi 保留，小何没有成为已选音色。
#[test]
fn doubao_menu_shift_tab_cancels_search_without_saving_browsed_voice() {
    assert_doubao_menu_tab_cancels("shift-tab");
}

fn assert_doubao_menu_tab_cancels(exit_key: &str) {
    use super::super::menu_state::VoiceMenuState;
    use crate::app::MultisoulApp;
    use gpui::{size, AppContext, TestAppContext};

    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let selected = find_voice("zh_female_vv_uranus_bigtts").expect("目录应有 Vivi");
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let mut menu = None;
    let mut app_entity = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        let state = cx
            .new(|cx| VoiceMenuState::new(Theme::default(), selected, app.downgrade(), window, cx));
        state.update(cx, |state, cx| state.show(window, cx));
        let body = state.read(cx).body.clone();
        menu = Some(state);
        app_entity = Some(app);
        ui::component::Root::new(body, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let menu = menu.expect("应建立音色菜单");
    let app = app_entity.expect("应保留真实 App 状态，避免弱引用失效掩盖保存问题");
    menu.update_in(cx, |menu, window, cx| {
        menu.list.update(cx, |list, cx| list.focus(window, cx));
    });
    assert!(
        menu.read_with(cx, |menu, _| menu.open),
        "退出前菜单必须已经打开"
    );
    cx.simulate_keystrokes("down");
    let browsed_id = menu.read_with(cx, |menu, cx| {
        let list = menu.list.read(cx);
        list.selected_index()
            .and_then(|index| list.delegate().voice_at(index.row))
            .map(|voice| voice.id.clone())
    });
    assert_eq!(
        browsed_id.as_deref(),
        Some("zh_female_xiaohe_uranus_bigtts"),
        "Down 必须实际浏览到小何，确保退出测试包含尚未确认的目标"
    );
    cx.simulate_keystrokes(exit_key);
    assert!(
        !menu.read_with(cx, |menu, _| menu.open),
        "Tab 不能被搜索输入吞掉而留下菜单"
    );
    cx.update(|window, cx| {
        assert!(
            menu.read(cx).trigger_focus.is_focused(window),
            "取消菜单后焦点必须返回音色字段"
        );
    });
    let selected_id = app.read_with(cx, |app, _| {
        app.voice_settings.values().doubao_voice().id.clone()
    });
    assert_eq!(
        selected_id, "zh_female_vv_uranus_bigtts",
        "Tab 退出必须保留 Vivi"
    );
    assert_ne!(
        selected_id, "zh_female_xiaohe_uranus_bigtts",
        "浏览过的小何不能未经确认成为已选音色"
    );
}

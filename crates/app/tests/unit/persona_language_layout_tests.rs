use super::*;
use gpui::{prelude::*, px, size, TestAppContext};

/// 英文人格说明在卡片内换行，不穿过相邻卡片。
///
/// 数据构造：窗口宽 780px，等于人格页最大宽度；三张卡片扣除两处 12px 间隔，平均 252px。
///   Warm 英文说明比中文长，一行宽度超过卡片内可用宽度；说明行高为 20px。
/// 执行过程：安装英文和浅色组件主题 → 建立独立 PersonaView → 绘制 → 读取说明与卡片边界。
/// 预期结果：说明为两到三行（40–60px），宽度至少 160px；不能逐字竖排或超出自身及相邻卡片。
#[test]
fn english_persona_descriptions_wrap_inside_cards() {
    let mut test = TestAppContext::single();
    let temp = tempfile::tempdir().expect("应有隔离人格存储");
    test.update(|cx| {
        ui::i18n::set_app_locale(ui::i18n::Locale::En, cx);
        ui::sync_component_theme(
            ui::ThemeId::ALL
                .into_iter()
                .find(|id| !id.is_dark())
                .expect("应有浅色主题"),
            cx,
        );
    });
    let (_, cx) = test.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            PersonaView::new(
                ui::Theme::for_id(
                    ui::ThemeId::ALL
                        .into_iter()
                        .find(|id| !id.is_dark())
                        .unwrap(),
                ),
                SettingsStore::new(temp.path().join("settings.sqlite")),
                window,
                cx,
            )
        });
        ui::component::Root::new(view, window, cx)
    });
    // 780px 与生产页面 max_w 相同；800px 高度给卡片和输入区域足够绘制空间。
    cx.simulate_resize(size(px(780.0), px(800.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let description = cx
        .debug_bounds("persona-tone-description-Warm")
        .expect("应绘制英文说明");
    let warm = cx
        .debug_bounds("persona-tone-Warm")
        .expect("应绘制 Warm 卡片");
    let next = cx
        .debug_bounds("persona-tone-Professional")
        .expect("应绘制相邻卡片");
    let summary = cx.debug_bounds("persona-summary").expect("应绘制人格简介");
    assert!(
        summary.size.width >= px(600.0),
        "简介必须使用徽标右侧空间，不能坍缩竖排：{summary:?}"
    );
    assert!(
        summary.size.height <= px(40.0),
        "简介最多两行，不能穿过下面的卡片：{summary:?}"
    );
    assert!(
        description.size.height > px(20.0),
        "英文长说明必须换行，不得强制单行：description={description:?}, warm={warm:?}"
    );
    assert!(
        description.size.width >= px(160.0),
        "说明必须使用卡片宽度，不能坍缩为逐字竖排：{description:?}"
    );
    assert!(
        description.size.height <= px(60.0),
        "说明最多三行，不能无限延伸：{description:?}"
    );
    assert!(
        description.bottom() <= warm.bottom(),
        "说明不能越过卡片底边：description={description:?}, warm={warm:?}"
    );
    assert!(
        description.right() <= warm.right(),
        "说明必须包含在自身卡片内"
    );
    assert!(
        description.right() < next.left(),
        "说明不能穿过卡片间隔进入相邻卡片"
    );
}

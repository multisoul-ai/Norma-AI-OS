use super::*;
use gpui::{prelude::*, Context, Entity, Render, TestAppContext, Window};

const TIDE: &str = "wallpapers/1403512.mp4";

struct WallpaperPageFixture {
    app: Entity<MultisoulApp>,
}

impl Render for WallpaperPageFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app.update(cx, |app, cx| {
            crate::settings::wallpaper::wallpaper_settings(
                app.theme,
                &app.wallpaper_settings,
                &mut app.wallpaper_previews,
                780.0,
                cx,
            )
        })
    }
}

// 场景：轻量包的视频尚未下载，当前仍选择默认静态图。
// 构造隔离数据库与仅有 Tide 的外置资源清单；渲染产品实际壁纸页面。
// 执行：后台初始化完成后替换测试媒体快照，再绘制实际素材库。
// 预期：未选中的 Tide 也有“需下载”状态；不需要先点击才发现资源缺失。
#[test]
fn wallpaper_library_exposes_download_before_selection() {
    let temp = tempfile::tempdir().expect("应创建隔离测试目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化测试数据库");
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let mut saved_app = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        saved_app = Some(app.clone());
        let page = cx.new(|_| WallpaperPageFixture { app });
        ui::component::Root::new(page, window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let json = serde_json::json!({
            "schema_version": 1, "resource_version": "feedback-test",
            "resources": [{ "path": TIDE, "url": "https://example.com/tide.mp4",
                "bytes": 3, "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "bundled": false }]
        });
        let mut library = SharedMedia::default();
        library.catalog = Some(Arc::new(MediaCatalog::from_json(&json.to_string()).expect("清单应合法")));
        cx.set_global(library);
        cx.refresh_windows();
        window.draw(cx).clear();
    });
    assert!(
        cx.debug_bounds("wallpaper-availability-missing-wallpapers/1403512.mp4")
            .is_some(),
        "实际素材库必须直接显示未选中壁纸的需下载状态"
    );
    assert!(
        cx.debug_bounds("media-action-wallpapers/1403512.mp4")
            .is_none(),
        "未选中 Tide 不应依赖当前壁纸区域的下载按钮提供反馈"
    );
    let app = saved_app.expect("应保留真实页面的 App 状态");
    app.update(cx, |app, cx| {
        let selected = crate::settings::wallpaper::state::WallpaperSelection::Builtin { rel: TIDE };
        app.wallpaper_settings.selection = selected.clone();
        app.wallpaper_settings.preferences.global = selected;
        cx.notify();
    });
    cx.update(|window, cx| {
        cx.refresh_windows();
        window.draw(cx).clear();
    });
    assert!(
        cx.debug_bounds("wallpaper-pending-selection").is_some(),
        "未下载的所选壁纸必须说明待应用，不能只标记为当前壁纸"
    );
    assert!(
        cx.debug_bounds("media-action-wallpapers/1403512.mp4")
            .is_some(),
        "所选壁纸必须提供可聚焦的下载操作"
    );
    // 场景：保存失败回滚到同样尚未下载的旧选择；错误不能因“待应用”而被隐藏。
    // 构造 Missing + 非空保存失败状态，重绘实际页面并检查保存反馈区域。
    // 预期：异常仍可见；清除异常后不虚构普通自动保存成功提示。
    app.update(cx, |app, _| {
        app.wallpaper_settings.preferences.status = Some("Couldn't save wallpaper".into())
    });
    cx.update(|window, cx| {
        cx.refresh_windows();
        window.draw(cx).clear();
    });
    assert!(
        cx.debug_bounds("wallpaper-save-feedback").is_some(),
        "待下载不能掩盖保存失败"
    );
    app.update(cx, |app, _| {
        app.wallpaper_settings.preferences.status = None
    });
    cx.update(|window, cx| {
        cx.refresh_windows();
        window.draw(cx).clear();
    });
    assert!(
        cx.debug_bounds("wallpaper-save-feedback").is_none(),
        "待下载且无保存结果时不应虚构成功提示"
    );

    // 场景：同一所选资源依次下载、收齐字节、失败、取消、完成。
    // 构造 3 字节可信文件；1/3 = 33%，3/3 仍需校验后才 Ready。
    // 执行实际页面重绘；预期每项状态出现在该素材下，且只有 Ready 移除待应用提示。
    let id = cx
        .update(|_, cx| {
            cx.global_mut::<SharedMedia>()
                .transfers
                .begin(TIDE, 3, true)
        })
        .expect("下载应开始");
    for (received, state) in [
        (0, "wallpaper-availability-preparing-wallpapers/1403512.mp4"),
        (
            1,
            "wallpaper-availability-downloading-wallpapers/1403512.mp4",
        ),
        (3, "wallpaper-availability-verifying-wallpapers/1403512.mp4"),
    ] {
        cx.update(|window, cx| {
            assert!(
                cx.global_mut::<SharedMedia>()
                    .transfers
                    .progress(TIDE, id, received),
                "当前下载进度应被接受"
            );
            cx.refresh_windows();
            window.draw(cx).clear();
        });
        assert!(
            cx.debug_bounds(state).is_some(),
            "实际页面应显示 {state} 状态"
        );
        assert!(
            cx.debug_bounds("wallpaper-pending-selection").is_some(),
            "未校验成功之前不得显示已应用"
        );
    }
    cx.update(|window, cx| {
        assert!(
            cx.global_mut::<SharedMedia>().transfers.finish(
                TIDE,
                id,
                Err("HTTP status client error (404 Not Found)".into())
            ),
            "下载失败应被接受"
        );
        cx.refresh_windows();
        window.draw(cx).clear();
    });
    assert!(
        cx.debug_bounds("wallpaper-availability-failed-wallpapers/1403512.mp4")
            .is_some(),
        "素材库应直接显示失败，而不是留下静默的已选标记"
    );
    cx.update(|window, cx| {
        let transfers = &mut cx.global_mut::<SharedMedia>().transfers;
        transfers.begin(TIDE, 3, true).expect("失败资源应能重试");
        assert!(transfers.cancel(TIDE), "下载应能取消");
        cx.refresh_windows();
        window.draw(cx).clear();
    });
    assert!(
        cx.debug_bounds("wallpaper-availability-cancelled-wallpapers/1403512.mp4")
            .is_some(),
        "取消应有持续状态反馈"
    );
    cx.update(|window, cx| {
        let transfers = &mut cx.global_mut::<SharedMedia>().transfers;
        let id = transfers.begin(TIDE, 3, true).expect("取消后应能重试");
        assert!(
            transfers.finish(TIDE, id, Ok(temp.path().join("verified.mp4"))),
            "可信安装完成应进入 Ready"
        );
        cx.refresh_windows();
        window.draw(cx).clear();
    });
    assert!(
        cx.debug_bounds("wallpaper-availability-ready-wallpapers/1403512.mp4")
            .is_some(),
        "下载成功后应显示已下载、可离线使用"
    );
    assert!(
        cx.debug_bounds("wallpaper-pending-selection").is_none(),
        "已下载后不得留下待应用提示"
    );
    assert!(
        cx.debug_bounds("wallpaper-availability-missing-wallpapers/1403512.mp4")
            .is_none(),
        "已下载资源不得继续显示需下载"
    );
}

/// 缓存命中的重试不应让已显示的壁纸闪回地球；失效后重新下载则应淡入。
///
/// 数据构造（含关键数值的推导过程）：
///   选择 74174.webp；缓存文件 3 bytes（"abc"）；两次传输分别从 Ready 和 Ready 后失效开始。
///   正常图片淡化为 220ms；完成事件对应新过渡的第 0ms。
///
/// 执行过程：
///   1. 预置已验证缓存，以 retry=true 开始传输并完成。
///   2. 再以 retry=true 开始传输，先发失效事件，再发完成事件。
///
/// 预期结果：
///   - 正断言：失效后的完成事件为当前图片建立从地球图淡出的过渡。
///   - 负断言：缓存命中的完成事件不得启动过渡或插入地球图。
#[test]
fn cached_wallpaper_retry_does_not_flash_fallback() {
    const REL: &str = "wallpapers/74174.webp";
    let temp = tempfile::tempdir().expect("应创建隔离测试目录");
    let path = temp.path().join("cached.webp");
    std::fs::write(&path, b"abc").expect("应创建测试缓存文件");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离数据库");
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let mut saved_app = None;
    let (_, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        saved_app = Some(app.clone());
        WallpaperPageFixture { app }
    });
    let app = saved_app.expect("应保留壁纸页面的 App 状态");
    cx.run_until_parked();
    app.update(cx, |app, _| {
        app.wallpaper_settings.selection =
            crate::settings::wallpaper::state::WallpaperSelection::Builtin { rel: REL };
    });

    cx.update(|_, cx| {
        let mut library = SharedMedia::default();
        library.subscribers.push(app.downgrade());
        library.transfers.seed_verified(REL, path.clone());
        let id = library
            .transfers
            .begin(REL, 3, true)
            .expect("Ready 图片应允许显式重试");
        cache::publish(REL, path.clone());
        cx.set_global(library);
        receive(REL, id, DownloadEvent::Finished(Ok(path.clone())), cx);
    });
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.wallpaper_settings.selection,
            crate::settings::wallpaper::state::WallpaperSelection::Builtin { rel: REL },
            "缓存命中后应继续显示所选图片"
        );
        assert!(
            !app.wallpaper_settings.transition_active(),
            "已显示的缓存图片不应闪回地球图"
        );
    });

    cx.update(|_, cx| {
        let id = cx
            .global_mut::<SharedMedia>()
            .transfers
            .begin(REL, 3, true)
            .expect("Ready 图片失效前应允许重试");
        receive(REL, id, DownloadEvent::CacheInvalidated, cx);
        receive(REL, id, DownloadEvent::Finished(Ok(path.clone())), cx);
    });
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.wallpaper_settings.transition_from(),
            Some(&crate::settings::wallpaper::state::WallpaperSelection::default()),
            "失效后重新下载应从地球回退画面淡入"
        );
        assert!(
            app.wallpaper_settings.transition_active(),
            "真实新资源到达时应启动图片过渡"
        );
    });
    cache::revoke(REL);
}

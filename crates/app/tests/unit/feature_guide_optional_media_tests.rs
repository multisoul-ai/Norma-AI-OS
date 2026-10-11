use super::{FeatureGuideTopic, MultisoulApp, OpenFeatureGuide};
use gpui::TestAppContext;
use media_store::MediaAvailability;

// 场景：教程下载完成后关闭，文件被外部删除，再打开同一主题。
// 用隔离数据库和 Ready 缓存目录构造失效路径；先让控制器记住同一主题。
// 预期显式重开进入后台校验，不能因主题未变而永久保持 Ready。
#[test]
fn reopening_the_same_guide_revalidates_ready_media() {
    let fixture = tempfile::tempdir().expect("isolated fixture must exist");
    let paths = storage::DatabasePaths::isolated_test(&fixture.path().join("database"));
    let store = storage::AppStore::initialize(&paths).expect("isolated database must open");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let topic = FeatureGuideTopic::VoiceGroupControl;
    let rel = topic
        .video_asset()
        .expect("fixture topic must have original video");
    app.update(cx, |app, cx| {
        app.feature_guide.open_manually();
        app.feature_guide.select_topic(topic);
        app.sync_feature_guide_media(cx);
        app.close_feature_guide_for_session(cx);
    });
    cx.update(|_, cx| crate::media::seed_verified_fixture(rel, fixture.path().join("cache"), cx));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_open_feature_guide(&OpenFeatureGuide, window, cx)
        });
        assert!(
            matches!(
                crate::media::availability(rel, cx),
                Some(MediaAvailability::Downloading { .. })
            ),
            "explicit same-topic reopen must leave Ready and revalidate in background"
        );
        crate::media::shutdown(cx);
    });
}

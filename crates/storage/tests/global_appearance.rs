use storage::{DesktopWorkspaceStore, DispatchEffectPreference, SettingsStore, WallpaperReference};

/// 场景：升级时采用最近打开空间中展开项目的外观，之后不再被旧字段覆盖。
/// 数据构造：旧空间、最近空间、一个展开项目，各有不同壁纸；项目有来源特效覆盖。
/// 执行过程：构造旧库并去掉迁移标记，读取设置触发升级，再改全局值并重新读取。
/// 预期结果：项目外观迁为全局；新值和撤销后的空值不被再次迁移覆盖；旧数据保留。
#[test]
fn global_appearance_migrates_latest_project_once() {
    let dir = tempfile::tempdir().expect("创建目录");
    let path = dir.path().join("app.sqlite");
    let catalog = DesktopWorkspaceStore::new(path.clone());
    let settings = SettingsStore::new(path.clone());
    let first = catalog
        .create_with_wallpaper(
            "Old",
            &WallpaperReference::Builtin {
                rel: "wallpapers/1331865.webp".into(),
            },
        )
        .expect("旧空间");
    let recent = catalog
        .create_with_wallpaper(
            "Recent",
            &WallpaperReference::Builtin {
                rel: "wallpapers/1358524.webp".into(),
            },
        )
        .expect("最近空间");
    let project = catalog.add_project(&recent.id, dir.path()).expect("项目");
    let wallpaper = WallpaperReference::Builtin {
        rel: "wallpapers/1350954.webp".into(),
    };
    catalog
        .set_project_wallpaper(&recent.id, &project.id, Some(&wallpaper))
        .expect("项目壁纸");
    let effect = DispatchEffectPreference {
        style: "ribbon".into(),
        voice: Some("fireworks".into()),
        idea: None,
        handoff: None,
    };
    catalog
        .set_project_dispatch_effect(&recent.id, &project.id, Some(&effect))
        .expect("项目特效");
    let connection = rusqlite::Connection::open(&path).expect("旧库连接");
    connection
        .execute(
            "UPDATE desktop_workspaces SET last_opened_at_unix_ms=1 WHERE id=?1",
            [&first.id],
        )
        .expect("旧时间");
    connection.execute("UPDATE desktop_workspaces SET last_opened_at_unix_ms=2, expanded_area_id=?2 WHERE id=?1", [&recent.id, &project.id]).expect("最近展开项目");
    connection
        .execute(
            "DELETE FROM settings WHERE key='appearance.global_migrated.v1'",
            [],
        )
        .expect("模拟升级前");
    drop(connection);
    assert_eq!(
        settings.get_string("wallpaper.value").expect("壁纸"),
        Some("wallpapers/1350954.webp".into()),
        "项目外观应迁为全局"
    );
    assert_eq!(
        settings.get_string("effects.dispatch.v1").expect("特效"),
        Some(serde_json::to_string(&effect).expect("JSON")),
        "来源覆盖应完整迁移"
    );
    settings
        .set_string("wallpaper.value", "wallpapers/1331865.webp")
        .expect("修改全局");
    settings
        .delete_string("effects.dispatch.v1")
        .expect("撤销全局特效");
    assert_eq!(
        settings.get_string("wallpaper.value").expect("重新打开"),
        Some("wallpapers/1331865.webp".into()),
        "重开不得再导入旧壁纸"
    );
    assert_eq!(
        settings
            .get_string("effects.dispatch.v1")
            .expect("重新打开"),
        None,
        "撤销后不能复活旧特效"
    );
    assert_eq!(
        catalog.list_projects(&recent.id).expect("旧项目")[0].wallpaper,
        Some(wallpaper),
        "迁移应保留旧数据"
    );
}

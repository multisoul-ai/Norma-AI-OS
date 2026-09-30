use super::*;

/// 文件编辑：保存 Unicode 多行文本后重新读取，内容完整更新。
///
/// 数据构造（含关键数值的推导过程）：
///   原文为两行 Markdown；新文本含中文、空行与末尾换行，验证逐字保存。
/// 执行过程：
///   1. 建立临时文件，读取原文作为保存基线。
///   2. 保存编辑后的文本，再通过预览读取接口打开文件。
/// 预期结果：
///   - 正断言：磁盘及预览返回完整新内容。
///   - 负断言：不得继续返回旧内容。
#[test]
fn preview_editing_saves_and_reloads_exact_text() {
    let dir = tempfile::tempdir().expect("创建临时目录");
    let path = dir.path().join("notes.md");
    let original = "# Original\ncontent\n";
    let edited = "# 修改后的标题\n\n正文 📝\n";
    std::fs::write(&path, original).expect("写入原始文件");

    save_text_file(&path, original, edited).expect("保存应成功");
    let loaded = crate::preview::load_preview_content(path.clone(), None).expect("重新读取文件");
    assert_eq!(
        std::fs::read_to_string(path).expect("读取磁盘"),
        edited,
        "保存必须保留完整文本及换行"
    );
    let crate::preview::PreviewContent::Markdown(document) = loaded else {
        panic!("保存后仍应使用 Markdown 预览");
    };
    assert_eq!(document.source, edited, "预览应显示保存后的内容");
    assert_ne!(document.source, original, "旧内容必须被替换");
}

/// 外部冲突：编辑期间原文件被其他程序修改时，保存不得覆盖外部内容。
///
/// 数据构造（含关键数值的推导过程）：
///   三个不同版本分别为 original、external、draft，不涉及数值预算。
/// 执行过程：
///   1. 文件已变为 external，仍以 original 为基线提交 draft。
///   2. 检查错误、磁盘内容与临时文件清理。
/// 预期结果：
///   - 正断言：返回外部修改提示，external 保留。
///   - 负断言：draft 不得落盘，也不得遗留临时文件。
#[test]
fn preview_editing_rejects_external_changes() {
    let dir = tempfile::tempdir().expect("创建临时目录");
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "external").expect("模拟外部修改");
    let error = save_text_file(&path, "original", "draft").expect_err("必须拒绝过期基线");
    assert!(
        error.contains("changed outside Norma"),
        "冲突必须提供明确提示"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("读取文件"),
        "external",
        "必须保留外部内容"
    );
    assert_ne!(
        std::fs::read_to_string(path).expect("读取文件"),
        "draft",
        "不得覆盖为草稿"
    );
    assert_eq!(
        std::fs::read_dir(dir.path()).expect("列出临时目录").count(),
        1,
        "失败后只保留原文件"
    );
}

/// 文件边界：只读或已删除的文件保存失败，不能改变文件权限或重建文件。
///
/// 数据构造（含关键数值的推导过程）：
///   Unix 权限 0o444 = 用户/组/其他各只读（4），无写位。
/// 执行过程：
///   1. 写入 original 并置为只读，尝试保存 draft。
///   2. 删除原文件后再次保存，模拟编辑过程中外部删除。
/// 预期结果：
///   - 正断言：两次保存都报错，首次文件内容和权限保留。
///   - 负断言：删除后不会隐式重建文件。
#[cfg(unix)]
#[test]
fn preview_editing_preserves_readonly_and_missing_files() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("创建临时目录");
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "original").expect("写入文件");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).expect("设为只读");
    assert!(
        save_text_file(&path, "original", "draft").is_err(),
        "只读文件应拒绝保存"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("读取文件"),
        "original",
        "只读原文不能被覆盖"
    );
    assert!(
        std::fs::metadata(&path)
            .expect("读取权限")
            .permissions()
            .readonly(),
        "不能绕过只读权限"
    );
    std::fs::remove_file(&path).expect("删除文件");
    assert!(
        save_text_file(&path, "original", "draft").is_err(),
        "丢失的源文件应报错"
    );
    assert!(!path.exists(), "不得重建已删除的文件");
}

/// 链接与空文件：通过符号链接保存空内容，链接及目标权限保持不变。
///
/// 数据构造（含关键数值的推导过程）：
///   目标权限 0o640 = 用户读写（6）+ 组只读（4）+ 其他无权限（0）。
///   edited = 空字符串，长度为 0，验证清空属于合法编辑。
/// 执行过程：
///   1. 创建指向目标的链接，读取源版本后通过链接保存空内容。
///   2. 检查目标长度、权限及链接身份。
/// 预期结果：
///   - 正断言：目标为空，权限仍为 0o640。
///   - 负断言：符号链接不得被普通文件替换。
#[cfg(unix)]
#[test]
fn preview_editing_keeps_symlink_and_permissions_when_clearing_file() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().expect("创建临时目录");
    let target = dir.path().join("target.txt");
    let link = dir.path().join("link.txt");
    std::fs::write(&target, "original").expect("创建目标文件");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).expect("设置权限");
    symlink(&target, &link).expect("创建链接");
    save_text_file(&link, "original", "").expect("允许清空文件");
    assert_eq!(
        std::fs::read_to_string(target.clone()).expect("读取目标"),
        "",
        "目标应为空"
    );
    assert_eq!(
        std::fs::metadata(target)
            .expect("读取权限")
            .permissions()
            .mode()
            & 0o777,
        0o640,
        "保存应保留权限位"
    );
    assert!(
        std::fs::symlink_metadata(&link)
            .expect("读取链接")
            .file_type()
            .is_symlink(),
        "原链接必须保留"
    );
    assert!(
        !std::fs::symlink_metadata(link)
            .expect("读取链接")
            .file_type()
            .is_file(),
        "不能用普通文件取代链接"
    );
}

/// 卡片编辑流程：编辑可保存和取消，未保存草稿不会被刷新或关闭丢弃。
///
/// 数据构造（含关键数值的推导过程）：
///   单张文本卡片，原文 original → 第一次草稿 saved → 第二次草稿 discarded。
///   卡片区域 640×480 仅提供布局，不参与文件内容判断；PTY 不应被创建。
/// 执行过程：
///   1. 使用生产读取与 Edit action 进入编辑，Input 更新触发未保存状态。
///   2. 尝试刷新、关闭和离开，验证草稿保留；触发 Save 后等待后台保存。
///   3. 再次修改，触发 Cancel，检查磁盘与阅读内容仍为第一次保存结果。
/// 预期结果：
///   - 正断言：保存更新磁盘和预览、清除 dirty；取消后可关闭。
///   - 负断言：未保存不能关闭或刷新，取消不会将第二次草稿写入磁盘。
#[test]
fn preview_editing_actions_preserve_drafts_and_save_then_cancel() {
    use crate::{
        app::MultisoulApp,
        preview::{PreviewContent, PreviewLoadState},
    };
    use gpui::{AppContext, TestAppContext};
    let dir = tempfile::tempdir().expect("创建临时目录");
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "original").expect("创建原文件");
    let mut gpui = TestAppContext::single();
    let mut app = None;
    let (_, cx) = gpui.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, _| panic!("预览不应创建 PTY")),
            )
        });
        app = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let app = app.expect("创建 App entity");
    let id = app.update(cx, |app, cx| {
        app.enter_canvas_for_test(dir.path(), cx);
        let id = app.runtime.spawn_preview_from_path(
            path.clone(),
            None,
            canvas::CanvasRect::new(0.0, 0.0, 640.0, 480.0),
        );
        app.refresh_preview_node(id, cx);
        id
    });
    cx.run_until_parked();
    let input = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_preview_edit_command(
                &PreviewEditCommand {
                    node_id: id,
                    command: PreviewEditCommandKind::Edit,
                },
                window,
                cx,
            );
            app.previews
                .editor_for(id)
                .expect("应进入编辑")
                .input
                .clone()
        })
    });
    cx.update(|window, cx| input.update(cx, |input, cx| input.set_value("saved", window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(app.previews.has_pending_edits(id), "修改文本后应标记未保存");
            app.refresh_preview_node(id, cx);
            assert_eq!(
                app.previews
                    .editor_for(id)
                    .expect("编辑仍在")
                    .input
                    .read(cx)
                    .value()
                    .as_ref(),
                "saved",
                "刷新不能覆盖草稿"
            );
            assert!(
                !app.close_node_through_route(id, window),
                "未保存时不能关闭卡片"
            );
            assert!(
                app.runtime.document.node(id).is_some(),
                "被阻止关闭的卡片应保留"
            );
            assert!(
                !app.confirm_preview_edits_before_leaving(cx),
                "离开前必须处理未保存内容"
            );
            app.save_preview_edit(id, cx);
        })
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert!(
            !app.previews.has_pending_edits(id),
            "保存成功应清除未保存状态"
        );
        assert!(
            app.previews.editor_for(id).expect("编辑仍在").saved,
            "应显示已保存"
        );
        assert_eq!(
            app.previews.state_for(id),
            Some(&PreviewLoadState::Ready(PreviewContent::Text(
                "saved".into()
            ))),
            "保存应更新阅读视图"
        );
        assert!(
            app.confirm_preview_edits_before_leaving(cx),
            "保存后允许离开"
        );
    });
    assert_eq!(
        std::fs::read_to_string(&path).expect("读取磁盘"),
        "saved",
        "Save action 必须真实写入文件"
    );
    cx.update(|window, cx| input.update(cx, |input, cx| input.set_value("discarded", window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_preview_edit_command(
                &PreviewEditCommand {
                    node_id: id,
                    command: PreviewEditCommandKind::Cancel,
                },
                window,
                cx,
            );
            assert!(app.previews.editor_for(id).is_none(), "取消应返回阅读视图");
            assert!(
                !app.previews.has_pending_edits(id),
                "取消不应遗留 dirty 状态"
            );
            assert!(
                app.close_node_through_route(id, window),
                "取消后应允许关闭卡片"
            );
            assert!(app.runtime.document.node(id).is_none(), "关闭后卡片应移除");
        })
    });
    assert_ne!(
        std::fs::read_to_string(path).expect("读取磁盘"),
        "discarded",
        "取消的修改不能写入磁盘"
    );
}

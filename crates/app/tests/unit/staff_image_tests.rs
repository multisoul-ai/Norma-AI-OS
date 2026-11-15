use super::super::error_message;
use super::{
    prepare_staff_clipboard_image, staff_can_send, staff_image_from_clipboard, staff_outgoing,
    store_picked_image,
};
use gpui::{div, prelude::*, px, size, AppContext, Render, TestAppContext, Window};
use norma::protocol::{compose_image_message, Task, TaskMessage};
use std::{fs, io::Write, path::Path};

struct ComposerHost {
    app: gpui::Entity<crate::app::MultisoulApp>,
}
impl Render for ComposerHost {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(self.app.update(cx, |app, cx| {
            app.ensure_staff_inputs(window, cx);
            let task = app.staff.tasks.values().next().cloned();
            let composer = app.staff_composer(None, "Project", window, cx);
            let message = task
                .as_ref()
                .map(|task| app.render_message(task, 0, &task.messages[0], window, cx));
            div().child(composer).children(message)
        }))
    }
}

fn png(dir: &Path, name: &str, width: u32, height: u32) -> std::path::PathBuf {
    let path = dir.join(name);
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        width,
        height,
        image::Rgb([9, 8, 7]),
    ))
    .save_with_format(&path, image::ImageFormat::Png)
    .expect("应写出 png");
    path
}

/// 场景：幕僚选择的图片写入既有 mobile-images 目录，坏图和超额有明确错误。
/// 数据构造：1×1 PNG；2 MiB+1 的空文件；非图片字节；2049×1 PNG；256 MiB 占位文件。
/// 数值：单张上限 2×1024×1024 字节，边长上限 2048，目录配额 256×1024×1024 字节。
/// 执行过程：调用与发送前相同的 store_picked_image / accept_chat_image。
/// 预期结果：合格图落在 `.mobile-images` 且可重复保存；四种失败分别映射到图片错误文案。
#[test]
fn staff_image_upload_uses_the_existing_store_and_reports_failures() {
    let dir = tempfile::tempdir().expect("临时目录");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let database = paths.app_db_path();
    let tiny = png(dir.path(), "tiny.png", 1, 1);
    let saved = store_picked_image(database, "ws", &tiny).expect("1×1 png 应保存");
    assert!(
        saved.contains(".mobile-images/ws/") && saved.ends_with(".png"),
        "应写入既有上传目录：{saved}"
    );
    assert_eq!(
        fs::read(&saved).expect("文件应存在"),
        fs::read(&tiny).expect("原图应存在"),
        "保存内容必须与所选文件一致"
    );
    assert_eq!(
        crate::mobile::accept_chat_image(database, "ws", &fs::read(&tiny).expect("读回"))
            .expect("相同字节应复用"),
        saved,
        "重复上传不能另存一份"
    );
    let desktop =
        "desktop:CDC3EA3E-CDEC-4FC8-82CC-C77D94607C8F:CE60ECF4-1891-41E2-9C05-32F8331E140D";
    let desktop_saved =
        crate::mobile::accept_chat_image(database, desktop, &fs::read(&tiny).expect("读回"))
            .expect("桌面工作区 id 含冒号时仍应保存");
    assert!(
        desktop_saved.contains(
            ".mobile-images/desktop_CDC3EA3E-CDEC-4FC8-82CC-C77D94607C8F_CE60ECF4-1891-41E2-9C05-32F8331E140D/"
        ) && !desktop_saved.contains(':'),
        "目录名应把冒号换成下划线，避免被当成非法图片：{desktop_saved}"
    );

    let mut huge = fs::File::create(dir.path().join("huge.bin")).expect("创建超限文件");
    huge.write_all(&[0]).expect("写入");
    huge.set_len(2 * 1024 * 1024 + 1).expect("超限长度");
    assert_eq!(
        store_picked_image(database, "ws", &dir.path().join("huge.bin")).expect_err("超过 2 MiB"),
        "invalid_image"
    );
    fs::write(dir.path().join("notes.txt"), b"hello").expect("非图片");
    assert_eq!(
        store_picked_image(database, "ws", &dir.path().join("notes.txt")).expect_err("非图片"),
        "invalid_image"
    );
    let wide = png(dir.path(), "wide.png", 2049, 1);
    assert_eq!(
        store_picked_image(database, "ws", &wide).expect_err("边长 2049 超过 2048"),
        "invalid_image"
    );

    let folder = database.with_extension("mobile-images").join("full");
    fs::create_dir_all(&folder).expect("配额目录");
    let pad = fs::File::create(folder.join("pad.bin")).expect("占位");
    pad.set_len(256 * 1024 * 1024).expect("占满 256 MiB 配额");
    assert_eq!(
        store_picked_image(database, "full", &tiny).expect_err("配额已满"),
        "image_storage_full"
    );

    assert_eq!(
        error_message("invalid_image".into()),
        ui::i18n::message("staff.image_invalid", &[]),
        "无法读取或超限必须显示图片错误"
    );
    assert_eq!(
        error_message("too_many_images".into()),
        ui::i18n::message("staff.image_too_many", &[]),
        "超过 4 张必须单独说明"
    );
    assert_eq!(
        error_message("image_storage_full".into()),
        ui::i18n::message("staff.image_storage_full", &[]),
        "空间已满不能显示成笼统的操作失败"
    );
    assert_ne!(
        error_message("image_storage_unavailable".into()),
        ui::i18n::message("staff.operation_failed", &[]),
        "保存失败要有自己的说明"
    );
}

/// 场景：有图无字也能发送，正文用既有图片协议，标题不露出协议。
/// 数据构造：一条合格路径；空正文；另一条带说明的正文。
/// 执行过程：staff_outgoing 与 staff_can_send。
/// 预期结果：纯图片可发送且解析回空正文；纯空白不可发送；上传中不可发送。
#[test]
fn staff_outgoing_message_carries_images_without_requiring_text() {
    let path = format!("/tmp/app.mobile-images/ws/{}.png", "ab".repeat(32));
    assert!(
        !staff_can_send(false, false, "  ", 0),
        "没有文字也没有图片时不能发送"
    );
    assert!(
        staff_can_send(false, false, "", 1),
        "只有图片时发送按钮应可用"
    );
    assert!(
        !staff_can_send(false, true, "在", 1),
        "上传未完成时不能发送"
    );
    let (body, title) = staff_outgoing("", &[path.clone()]).expect("纯图片应能发送");
    let (visible, paths) = norma::protocol::parse_image_message(&body).expect("应能解析");
    assert_eq!(visible, "", "协议外壳不是用户正文");
    assert_eq!(paths, vec![path.clone()], "发送体必须带上保存路径");
    // 存储文件名是 64 位哈希加扩展名，共 68 个字符；任务标题沿用原有 60 字上限。
    let filename = path.rsplit('/').next().expect("路径应有文件名");
    assert_eq!(
        title,
        filename.chars().take(60).collect::<String>(),
        "没有正文时标题用文件名前 60 个字符，不能改成协议标记"
    );
    assert!(!title.contains("norma-images"), "标题不能是协议标记");
    let (captioned, caption_title) =
        staff_outgoing("看这张图", &[path]).expect("带说明的图片应能发送");
    assert!(captioned.contains("看这张图"), "有文字时正文仍包含用户说明");
    assert_eq!(
        caption_title, "看这张图",
        "有文字时标题用第一行，不用文件名"
    );
    assert_eq!(
        staff_outgoing("  ", &[]).expect_err("空白不能发送"),
        "empty"
    );
}

/// 场景：复制图片后粘贴，按聊天输入的方式收成附件，不把文件路径写进正文。
/// 数据构造：1×1 PNG；3000×2 PNG；只含「screenshot.png」的剪贴板；系统图片加文件名条目。
/// 数值：边长上限 2048。3000 按 2048/3000 缩小后宽为 2048，高为 floor(2×2048/3000)=1。
/// 执行过程：prepare_staff_clipboard_image 与 staff_image_from_clipboard。
/// 预期结果：小图可写入既有存储；超宽图落在 2048 以内；纯文字不拦截；有图片时优先图片。
#[test]
fn staff_pasted_image_fits_the_store_and_does_not_become_text() {
    let tiny = encode_png(1, 1);
    let prepared = prepare_staff_clipboard_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        tiny.clone(),
    ))
    .expect("1×1 png 应可粘贴");
    assert_eq!(prepared, tiny, "未超限的 PNG 应原样保存，避免再压缩");
    let dir = tempfile::tempdir().expect("临时目录");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let saved = crate::mobile::accept_chat_image(paths.app_db_path(), "ws", &prepared)
        .expect("粘贴结果应走既有上传");
    assert!(
        saved.contains(".mobile-images/ws/") && saved.ends_with(".png"),
        "粘贴图片应进入既有目录：{saved}"
    );

    let wide = prepare_staff_clipboard_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        encode_png(3000, 2),
    ))
    .expect("超宽截图应缩小后仍可粘贴");
    let decoded = image::load_from_memory(&wide).expect("缩小结果应是图片");
    assert!(
        decoded.width() <= 2048 && decoded.height() <= 2048 && decoded.width() >= 1,
        "粘贴后边长必须落在存储上限内：{}×{}",
        decoded.width(),
        decoded.height()
    );
    assert!(wide.len() <= 2 * 1024 * 1024, "粘贴结果不能超过 2 MiB");

    let filename = gpui::ClipboardItem::new_string("screenshot.png".into());
    assert!(
        staff_image_from_clipboard(None, Some(&filename)).is_none(),
        "只有文字时必须继续粘贴到输入框"
    );
    let shot = gpui::Image::from_bytes(gpui::ImageFormat::Png, tiny);
    assert!(
        staff_image_from_clipboard(Some(shot), Some(&filename)).is_some(),
        "剪贴板同时有文件名和图片时应收成附件，不能把路径写进正文"
    );
}

fn encode_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        width,
        height,
        image::Rgb([9, 8, 7]),
    ))
    .write_to(&mut bytes, image::ImageFormat::Png)
    .expect("应编码 png");
    bytes.into_inner()
}

/// 场景：幕僚输入区能挂上已选图片，用户气泡展示图片而不是协议原文。
/// 数据构造：800×600 窗口、一条已保存路径、用户消息「看这张」加该路径。
/// 执行过程：生产 composer 与消息渲染器各画一帧。
/// 预期结果：添加按钮和图片条存在；气泡里有图片槽，协议标记不作为唯一正文。
#[test]
fn staff_composer_shows_the_attach_control_and_selected_image() {
    let dir = tempfile::tempdir().expect("临时目录");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let saved = store_picked_image(
        paths.app_db_path(),
        "ws",
        &png(dir.path(), "logo.png", 1, 1),
    )
    .expect("图片应先保存");
    let mut test = TestAppContext::single();
    let mut source = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| {
            crate::app::MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        source = Some(app.clone());
        let host = cx.new(|_| ComposerHost { app });
        ui::component::Root::new(host, window, cx)
    });
    let source = source.expect("app");
    cx.simulate_resize(size(px(800.), px(600.)));
    let mut task = Task::new("t", "w", "p", "图");
    task.messages.push(TaskMessage::User {
        text: compose_image_message("看这张", &[saved.clone()]).expect("打包"),
    });
    source.update(cx, |app, _| {
        app.staff.images = vec![saved];
        app.staff.tasks.insert(task.id.clone(), task);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("staff-attach-image").is_some(),
        "幕僚输入区必须有添加图片按钮"
    );
    let chip = cx
        .debug_bounds("staff-image-0")
        .expect("已选图片应出现在输入区");
    assert!(
        chip.size.width > px(90.) && chip.size.height > px(60.),
        "预览应是缩略图，而不是带文字的小条：{chip:?}"
    );
    let remove = cx
        .debug_bounds("staff-image-remove-0")
        .expect("缩略图角上应有移除按钮");
    assert!(
        remove.origin.x >= chip.origin.x
            && remove.origin.x < chip.origin.x + chip.size.width
            && remove.origin.y >= chip.origin.y
            && remove.origin.y < chip.origin.y + chip.size.height,
        "移除按钮应压在缩略图内部右上角：{remove:?} / {chip:?}"
    );
    let image = cx
        .debug_bounds("staff-user-image-0-0")
        .expect("用户气泡应显示图片");
    assert!(
        image.size.height > px(40.),
        "图片槽应占住高度，而不是协议文本"
    );
}

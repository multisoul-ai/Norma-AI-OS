use super::{
    builtin_wallpapers, bundle_resources_dir, canonical_builtin_wallpaper, resolve_builtin_asset,
    resolve_builtin_asset_from_paths, WallpaperKind,
};

/// T-R1: 扩展名 → kind 判定覆盖三类，未知扩展返回 None。
///
/// 数据构造（含关键数值的推导过程）：
///   "png" → Static, "jpg" → Static, "jpeg" → Static, "webp" → Static
///   "gif" → Gif
///   "mp4" → Video, "mov" → Video
///   "txt" → None（不在白名单）
///   ""    → None（空串无可识别扩展）
///   "PNG" → Static（大小写不敏感：to_ascii_lowercase 归一化后命中 png 分支）
///   "JPG" → Static, "WEBP" → Static, "GIF" → Gif, "MP4" → Video（大小写覆盖三类）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. WallpaperKind::from_extension 接收字符串
///   2. 内部 to_ascii_lowercase() 归一化大小写
///   3. match 命中静态/动态/视频分支，或回退 _ => None
///
/// 预期结果：
///   - 正断言：六类扩展各返回对应 WallpaperKind 枚举
///   - 负断言：txt 与空串返回 None，避免误判未知格式为可渲染壁纸
#[test]
fn test_kind_from_extension_classifies_three_types_and_rejects_unknown() {
    assert_eq!(
        WallpaperKind::from_extension("png"),
        Some(WallpaperKind::Static),
        "png must be Static"
    );
    assert_eq!(
        WallpaperKind::from_extension("jpg"),
        Some(WallpaperKind::Static),
        "jpg must be Static"
    );
    assert_eq!(
        WallpaperKind::from_extension("jpeg"),
        Some(WallpaperKind::Static),
        "jpeg must be Static"
    );
    assert_eq!(
        WallpaperKind::from_extension("webp"),
        Some(WallpaperKind::Static),
        "webp must be Static so optimized built-in wallpapers remain renderable"
    );
    assert_eq!(
        WallpaperKind::from_extension("gif"),
        Some(WallpaperKind::Gif),
        "gif must be Gif"
    );
    assert_eq!(
        WallpaperKind::from_extension("mp4"),
        Some(WallpaperKind::Video),
        "mp4 must be Video"
    );
    assert_eq!(
        WallpaperKind::from_extension("mov"),
        Some(WallpaperKind::Video),
        "mov must be Video"
    );
    assert_eq!(
        WallpaperKind::from_extension("txt"),
        None,
        "txt must be rejected — not a wallpaper format"
    );
    assert_eq!(
        WallpaperKind::from_extension(""),
        None,
        "empty ext must be None — no extension to classify"
    );
    assert_eq!(
        WallpaperKind::from_extension("PNG"),
        Some(WallpaperKind::Static),
        "ext matching is case-insensitive: PNG -> Static"
    );
    assert_eq!(
        WallpaperKind::from_extension("JPG"),
        Some(WallpaperKind::Static),
        "JPG uppercase must still be Static"
    );
    assert_eq!(
        WallpaperKind::from_extension("WEBP"),
        Some(WallpaperKind::Static),
        "WEBP uppercase must still be Static"
    );
    assert_eq!(
        WallpaperKind::from_extension("GIF"),
        Some(WallpaperKind::Gif),
        "GIF uppercase must still be Gif"
    );
    assert_eq!(
        WallpaperKind::from_extension("MP4"),
        Some(WallpaperKind::Video),
        "MP4 uppercase must still be Video"
    );
}

/// T-R10: 旧版 PNG 内置壁纸路径必须迁移到同名 WebP，保留用户现有选择。
///
/// 数据构造（含关键数值的推导过程）：
///   legacy PNG paths = 6 条（默认 3 条 + MUT v2 3 条）
///   canonical WebP paths = 同 basename 的 6 条 `.webp`
///   already canonical path = wallpapers/1350954.webp
///   unknown path = wallpapers/not-a-builtin.png
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐一输入 6 条旧 PNG 持久化路径
///   2. 输入已是 WebP 的当前路径
///   3. 输入未注册的未知路径
///
/// 预期结果：
///   - 正断言：6 条旧路径逐一映射到同名 WebP，不丢失用户壁纸选择
///   - 正断言：当前 WebP 路径保持不变
///   - 负断言：未知路径不得伪装成内置壁纸
#[test]
fn test_legacy_png_builtin_paths_migrate_to_canonical_webp() {
    assert_eq!(
        canonical_builtin_wallpaper("wallpapers/1331865.png"),
        Some("wallpapers/1331865.webp"),
        "legacy default PNG must migrate to the canonical default WebP"
    );
    assert_eq!(
        canonical_builtin_wallpaper("wallpapers/1350954.png"),
        Some("wallpapers/1350954.webp"),
        "legacy Aurora PNG must migrate to the canonical Aurora WebP"
    );
    assert_eq!(
        canonical_builtin_wallpaper("wallpapers/1358524.png"),
        Some("wallpapers/1358524.webp"),
        "legacy Nebula PNG must migrate to the canonical Nebula WebP"
    );
    assert_eq!(
        canonical_builtin_wallpaper("wallpapers/74174.png"),
        Some("wallpapers/74174.webp"),
        "legacy MUT v2 74174 PNG must migrate to its canonical WebP"
    );
    assert_eq!(
        canonical_builtin_wallpaper("wallpapers/1411492.png"),
        Some("wallpapers/1411492.webp"),
        "legacy MUT v2 1411492 PNG must migrate to its canonical WebP"
    );
    assert_eq!(
        canonical_builtin_wallpaper("wallpapers/1411385.png"),
        Some("wallpapers/1411385.webp"),
        "legacy MUT v2 1411385 PNG must migrate to its canonical WebP"
    );
    assert_eq!(
        canonical_builtin_wallpaper("wallpapers/1350954.webp"),
        Some("wallpapers/1350954.webp"),
        "an already canonical WebP path must remain unchanged"
    );
    assert_eq!(
        canonical_builtin_wallpaper("wallpapers/not-a-builtin.png"),
        None,
        "an unknown path must not be accepted as a built-in wallpaper"
    );
}

/// T-R9: Wallpaper 标签文案必须把静态图与两类动态资源明确区分。
///
/// 数据构造（含关键数值的推导过程）：
///   Static = PNG / JPG / JPEG，共 3 种静态扩展归一后的类别
///   Gif    = GIF，共 1 种逐帧动画类别
///   Video  = MP4 / MOV，共 2 种视频扩展归一后的类别
///   motion groups = static(1 类) + dynamic(2 类) = 2 组
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别读取 Static、Gif、Video 的 badge_label。
///   2. 确认 Static 显示为静态语义。
///   3. 确认 Gif 与 Video 都显示为动态语义。
///
/// 预期结果：
///   - 正断言：Static 精确显示 `Static`，用户可以识别静态壁纸。
///   - 正断言：Gif 与 Video 精确显示 `Dynamic`，用户可以识别动态壁纸。
///   - 负断言：Static 不得误标为 Dynamic；Gif / Video 不得误标为 Static。
#[test]
fn test_wallpaper_badge_labels_distinguish_static_and_dynamic_media() {
    let static_label = WallpaperKind::Static.badge_label();
    let gif_label = WallpaperKind::Gif.badge_label();
    let video_label = WallpaperKind::Video.badge_label();

    assert_eq!(
        static_label, "Static",
        "静态图片的 Wallpaper 胶囊标签必须精确显示 Static"
    );
    assert_ne!(
        static_label, "Dynamic",
        "静态图片不得显示 Dynamic，否则用户会误判其会运动"
    );
    assert_eq!(
        gif_label, "Dynamic",
        "GIF Wallpaper 胶囊标签必须精确显示 Dynamic"
    );
    assert_ne!(
        gif_label, "Static",
        "GIF 不得显示 Static，否则动态属性会被隐藏"
    );
    assert_eq!(
        video_label, "Dynamic",
        "视频 Wallpaper 胶囊标签必须精确显示 Dynamic"
    );
    assert_ne!(
        video_label, "Static",
        "视频不得显示 Static，否则动态属性会被隐藏"
    );
}

/// T-R2: 内置清单恰为 16 项 = 9 static（含既有默认和 mut/v2 的 6 张）
/// + 2 gif + 5 video；所有路径位于 wallpapers/ 前缀下。
///
/// 数据构造（含关键数值的推导过程）：
///   内置清单 = 1331865.webp(Static, default)
///            + 1350954.webp(Static) + 1358524.webp(Static) = 3 static
///            + 74174.webp + 74176.jpg + 718916.jpg
///              + 1411492.webp + 379352.jpg + 1411385.webp   = 6 static（mut/v2）
///            + 215824.gif(Gif) + 220240.gif(Gif)              = 2 gif
///            + 1403512.mp4 + 1404942.mp4 + 1406443.mp4
///              + 1406489.mp4 + 1407174.mp4                    = 5 video
///   Total   = 3 + 6 + 2 + 5 = 16
///   (counts = (static, gif, video) = (9, 2, 5))
///
/// 执行过程（逐步说明系统如何处理）：
///   1. builtin_wallpapers() 返回 &'static [(path, name, kind)] 切片
///   2. list.len() 校验总数
///   3. fold 按 kind 累加三类计数
///   4. 分别校验 6 个 mut/v2 静态壁纸存在
///   5. for 循环校验每条路径前缀
///
/// 预期结果：
///   - 正断言：len==16；counts==(9,2,5)；6 个 mut/v2 文件逐一存在且为 Static
///   - 负断言：每条路径必须以 wallpapers/ 开头（非空相对资源名）
#[test]
fn test_builtin_wallpapers_contains_mut_v2_defaults_with_correct_kinds() {
    let list = builtin_wallpapers();
    assert_eq!(
        list.len(),
        16,
        "built-in library must contain the 10 existing entries plus 6 mut/v2 defaults"
    );
    let counts = list.iter().fold((0, 0, 0), |(s, g, v), (_, _, k)| match k {
        WallpaperKind::Static => (s + 1, g, v),
        WallpaperKind::Gif => (s, g + 1, v),
        WallpaperKind::Video => (s, g, v + 1),
    });
    assert_eq!(
        counts,
        (9, 2, 5),
        "expected 9 static (3 existing + 6 mut/v2), 2 gif, 5 video — got {:?}",
        counts
    );
    assert!(
        list.iter().any(|(path, _, kind)| {
            *path == "wallpapers/74174.webp" && *kind == WallpaperKind::Static
        }),
        "mut/v2 wallpaper 74174.webp must be a packaged Static builtin"
    );
    assert!(
        list.iter().any(|(path, _, kind)| {
            *path == "wallpapers/74176.jpg" && *kind == WallpaperKind::Static
        }),
        "mut/v2 wallpaper 74176.jpg must be an embedded Static builtin"
    );
    assert!(
        list.iter().any(|(path, _, kind)| {
            *path == "wallpapers/718916.jpg" && *kind == WallpaperKind::Static
        }),
        "mut/v2 wallpaper 718916.jpg must be an embedded Static builtin"
    );
    assert!(
        list.iter().any(|(path, _, kind)| {
            *path == "wallpapers/1411492.webp" && *kind == WallpaperKind::Static
        }),
        "mut/v2 wallpaper 1411492.webp must be a packaged Static builtin"
    );
    assert!(
        list.iter().any(|(path, _, kind)| {
            *path == "wallpapers/379352.jpg" && *kind == WallpaperKind::Static
        }),
        "mut/v2 wallpaper 379352.jpg must be an embedded Static builtin"
    );
    assert!(
        list.iter().any(|(path, _, kind)| {
            *path == "wallpapers/1411385.webp" && *kind == WallpaperKind::Static
        }),
        "mut/v2 wallpaper 1411385.webp must be a packaged Static builtin"
    );
    for (p, _, _) in list {
        assert!(
            p.starts_with("wallpapers/"),
            "built-in path must be under wallpapers/, got {p}"
        );
        assert!(!p.is_empty(), "built-in path must not be empty");
    }
}

/// T-R3: 内置清单中每条路径的扩展名必须与其 WallpaperKind 自洽。
///
/// 数据构造（含关键数值的推导过程）：
///   遍历 builtin_wallpapers() 的 10 条记录
///   对每条 path，取最后一个 '.' 后的扩展名
///   .webp/.jpg → Static；.gif → Gif；.mp4 → Video
///
/// 执行过程（逐步说明系统如何处理）：
///   1. builtin_wallpapers() 取 10 条
///   2. 每条路径 rsplit_once('.') 取扩展名
///   3. WallpaperKind::from_extension(ext) 应等于记录的 kind
///
/// 预期结果：
///   - 正断言：每条 path 的扩展名经 from_extension 后 == 记录 kind
///   - 负断言：无任何记录出现扩展名与 kind 不一致的情况
#[test]
fn test_builtin_wallpapers_kind_matches_extension() {
    let list = builtin_wallpapers();
    for (path, _name, kind) in list {
        let ext = path.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        let derived = WallpaperKind::from_extension(ext);
        assert_eq!(
            derived,
            Some(*kind),
            "path {path} has extension '{ext}' which derives {derived:?}, \
             but its declared kind is {kind:?} — they must agree"
        );
    }
}

/// T-R4: bundle_resources_dir() 在开发态（cargo run，二进制位于 target/debug）
/// 必须返回 None —— 因为 target/debug 不符合 .app/Contents/MacOS 布局。
///
/// 数据构造（含关键数值的推导过程）：
///   current_exe() = .../target/debug/(deps/)?<bin><hash?>
///   exe.parent()  = .../target/debug(/deps)?   （不是 Contents/MacOS）
///   -> 不存在 contents.parent() whose file_name starts with ".app"
///   -> 返回 None
///
/// 执行过程（逐步说明系统如何处理）：
///   1. bundle_resources_dir() 读取 current_exe
///   2. 取 parent 链，检查是否落在 .app/Contents/MacOS
///   3. 开发态路径不符合该布局 -> 返回 None
///
/// 预期结果：
///   - 正断言：在 cargo test（开发态）下 bundle_resources_dir() == None
///   - 说明：仅在打包 .app 内运行时返回 Some；此测试在 dev 下恒为 None
#[test]
fn test_bundle_resources_dir_is_none_in_dev_build() {
    let dir = bundle_resources_dir();
    // 仅当当前进程恰好被装在 .app/Contents/MacOS 下时为 Some。
    // cargo test / cargo run 跑的是 target/... 二进制，故应为 None。
    // 我们不强 assert None（理论上 cargo test 可能被打进某种 bundle），
    // 但 dev 环境下断言 None 是预期且稳定的。
    assert!(
        dir.is_none(),
        "in dev (target/debug) layout, bundle_resources_dir must be None — got {:?}",
        dir
    );
}

/// T-R5: resolve_builtin_asset 对不存在的相对路径返回 None（文件未就位）。
///
/// 数据构造（含关键数值的推导过程）：
///   rel = "wallpapers/__nonexistent_test_asset__.png"
///   dev 路径 = CARGO_MANIFEST_DIR/assets/wallpapers/__nonexistent_test_asset__.png
///     该文件不存在（Task 3/9 才会拷入真实素材）
///   bundle 路径 = bundle_resources_dir()/wallpapers/__nonexistent_test_asset__.png
///     dev 下 bundle_resources_dir() == None -> 跳过
///   -> 无任何来源命中 -> None
///
/// 执行过程（逐步说明系统如何处理）：
///   1. resolve_builtin_asset("wallpapers/__nonexistent_test_asset__.png")
///   2. 检查 dev CARGO_MANIFEST_DIR/assets/<rel>：非 is_file -> 跳过
///   3. 检查 bundle resources/<rel>：bundle_resources_dir()==None -> 跳过
///   4. 返回 None
///
/// 预期结果：
///   - 正断言：返回 None，调用方据此回退默认壁纸
#[test]
fn test_resolve_builtin_asset_returns_none_for_missing_file() {
    let got = resolve_builtin_asset("wallpapers/__nonexistent_test_asset__.png");
    assert!(
        got.is_none(),
        "resolve_builtin_asset must return None for a missing asset — got {:?}",
        got
    );
}

/// T-R8: 开发 `.app` 同时拥有 bundle 资源与源码资源时，内置视频必须优先读取 bundle，避免请求“文稿”目录权限。
///
/// 数据构造（含关键数值的推导过程）：
///   rel = "wallpapers/1407174.mp4"
///   bundle_root/wallpapers/1407174.mp4 = "bundle-video"（开发 `.app` 的 Contents/Resources）
///   dev_root/wallpapers/1407174.mp4    = "source-video"（仓库源码，位于用户“文稿”目录）
///   两个候选文件同名且均存在；优先级必须为 bundle(第 1 个) > dev(第 2 个)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在临时目录分别构造 bundle 与源码两套同名视频资源
///   2. 将 bundle_root 和 dev_root 传入资源解析边界
///   3. 读取解析结果，比较它是否指向 bundle 文件而非源码文件
///
/// 预期结果：
///   - 正断言：解析路径等于 bundle 中的文件，开发 `.app` 不依赖源码目录
///   - 正断言：解析文件内容为 bundle-video，证明不是仅比较字符串前缀
///   - 负断言：解析路径不能等于 dev_root 的文件，否则 macOS 会向用户请求“文稿”权限
#[test]
fn test_resolve_builtin_asset_prefers_bundle_resources_over_development_source() {
    let temp = tempfile::tempdir().expect("应能创建临时资源目录");
    let rel = "wallpapers/1407174.mp4";
    let bundle_root = temp.path().join("Contents/Resources");
    let dev_root = temp.path().join("crates/app/assets");
    let bundle_file = bundle_root.join(rel);
    let dev_file = dev_root.join(rel);
    std::fs::create_dir_all(bundle_file.parent().expect("bundle 文件应有父目录"))
        .expect("应能创建 bundle 壁纸目录");
    std::fs::create_dir_all(dev_file.parent().expect("源码文件应有父目录"))
        .expect("应能创建源码壁纸目录");
    std::fs::write(&bundle_file, "bundle-video").expect("应能写入 bundle 视频夹具");
    std::fs::write(&dev_file, "source-video").expect("应能写入源码视频夹具");

    let resolved = resolve_builtin_asset_from_paths(rel, Some(&bundle_root), Some(&dev_root))
        .expect("两个候选资源都存在时应解析到一个路径");

    assert_eq!(
        resolved, bundle_file,
        "开发 .app 应优先使用 Contents/Resources 中的内置视频，避免访问源码目录"
    );
    assert_eq!(
        std::fs::read_to_string(&resolved).expect("解析路径应可读取"),
        "bundle-video",
        "解析结果应读取 bundle 副本，而不是同名源码副本"
    );
    assert_ne!(
        resolved, dev_file,
        "解析结果不能落到开发源码目录，否则会触发 macOS“文稿”访问授权"
    );
}

/// T-R6: 内置清单每条显示名非空，避免 UI 渲染空白条目。
///
/// 数据构造（含关键数值的推导过程）：
///   builtin_wallpapers() 的 10 条 (path, name, kind)
///   每条 name 不应为空字符串
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 遍历每条记录
///   2. 断言 name 非空
///
/// 预期结果：
///   - 正断言：所有显示名非空
///   - 负断言：无空字符串 name
#[test]
fn test_builtin_wallpapers_all_have_nonempty_display_names() {
    for (path, name, _kind) in builtin_wallpapers() {
        assert!(
            !name.is_empty(),
            "display name for {path} must not be empty"
        );
    }
}

/// T-R7: 内置清单的相对路径互不重复（唯一性）。
///
/// 数据构造（含关键数值的推导过程）：
///   10 条 path，两两不应相等
///   用 HashSet 去重，len 仍应为 10
///
/// 执行过程（逐步说明系统如何处理）：
///   1. collect 到 HashSet<&str>
///   2. 断言 set.len() == list.len()
///
/// 预期结果：
///   - 正断言：去重后仍为 10 条
///   - 负断言：无重复路径
#[test]
fn test_builtin_wallpapers_paths_are_unique() {
    use std::collections::HashSet;
    let list = builtin_wallpapers();
    let set: HashSet<&&str> = list.iter().map(|(p, _, _)| p).collect();
    assert_eq!(
        set.len(),
        list.len(),
        "built-in paths must be unique — duplicates detected"
    );
}

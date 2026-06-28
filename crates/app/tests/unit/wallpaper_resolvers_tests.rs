use super::wallpaper_resolvers::{builtin_wallpapers, resolve_builtin_asset, WallpaperKind};

/// T-R1: 扩展名 → kind 判定覆盖三类，未知扩展返回 None。
///
/// 数据构造（含关键数值的推导过程）：
///   "png" → Static, "jpg" → Static, "jpeg" → Static
///   "gif" → Gif
///   "mp4" → Video, "mov" → Video
///   "txt" → None（不在白名单）
///   ""    → None（空串无可识别扩展）
///   "PNG" → Static（大小写不敏感：to_ascii_lowercase 归一化后命中 png 分支）
///   "JPG" → Static, "GIF" → Gif, "MP4" → Video（大小写覆盖三类）
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

/// T-R2: 内置清单恰为 10 项 = 3 static (incl. default) + 2 gif + 5 video；
/// 所有路径位于 wallpapers/ 前缀下；默认壁纸 1331865.png 必须出现。
///
/// 数据构造（含关键数值的推导过程）：
///   内置清单 = 1331865.png(Static, default)
///            + 1350954.png(Static) + 1358524.png(Static)     = 3 static
///            + 215824.gif(Gif) + 220240.gif(Gif)              = 2 gif
///            + 1403512.mp4 + 1404942.mp4 + 1406443.mp4
///              + 1406489.mp4 + 1407174.mp4                    = 5 video
///   Total   = 3 + 2 + 5 = 10
///   (counts = (static, gif, video) = (3, 2, 5))
///
/// 执行过程（逐步说明系统如何处理）：
///   1. builtin_wallpapers() 返回 &'static [(path, name, kind)] 切片
///   2. list.len() 校验总数
///   3. fold 按 kind 累加三类计数
///   4. iter().any(...) 校验默认壁纸存在
///   5. for 循环校验每条路径前缀
///
/// 预期结果：
///   - 正断言：len==10；counts==(3,2,5)；含 1331865.png
///   - 负断言：每条路径必须以 wallpapers/ 开头（非空相对资源名）
#[test]
fn test_builtin_wallpapers_contains_exactly_ten_with_correct_kinds() {
    let list = builtin_wallpapers();
    assert_eq!(
        list.len(),
        10,
        "built-in library must contain exactly 10 entries"
    );
    let counts = list.iter().fold((0, 0, 0), |(s, g, v), (_, _, k)| match k {
        WallpaperKind::Static => (s + 1, g, v),
        WallpaperKind::Gif => (s, g + 1, v),
        WallpaperKind::Video => (s, g, v + 1),
    });
    assert_eq!(
        counts,
        (3, 2, 5),
        "expected 3 static (incl. default), 2 gif, 5 video — got {:?}",
        counts
    );
    assert!(
        list.iter().any(|(p, _, _)| p.ends_with("1331865.png")),
        "default wallpaper 1331865.png must be present"
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
///   .png → Static；.gif → Gif；.mp4 → Video
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
    use super::wallpaper_resolvers::bundle_resources_dir;
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

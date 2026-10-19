use super::*;
use std::path::Path;

fn facts(size: u64) -> PathFacts {
    PathFacts {
        is_dir: false,
        size,
        binary: false,
    }
}

/// 终端路径分流：md/图片进预览标签；项目内小文本进工作台；其余交给系统应用。
///
/// 数据构造（含关键数值的推导过程）：
///   MAX = MAX_TEXT_BYTES = 1 MiB = 1_048_576 bytes（与工作台读取上限一致，超过则 read_text 必然报错）
///   README.md（项目内）、shot.png（项目外）→ 预览标签
///   src/lib.rs 大小 2_048 bytes ≤ MAX、项目内、非二进制 → 工作台
///   notes.txt 在项目外 → 仍可由预览标签（txt 受支持）打开
///   main.rs 在项目外 → 无预览类型，系统应用
///
/// 执行过程：对每种组合调用纯函数 `route_terminal_path`。
///
/// 预期结果：见各断言。
#[test]
fn test_route_terminal_path_prefers_preview_for_markdown_and_images_and_workbench_for_project_text()
{
    let md = route_terminal_path(Path::new("/p/README.md"), &facts(10), true);
    let png = route_terminal_path(Path::new("/tmp/shot.png"), &facts(10), false);
    let rs = route_terminal_path(Path::new("/p/src/lib.rs"), &facts(2_048), true);
    let txt_outside = route_terminal_path(Path::new("/tmp/notes.txt"), &facts(10), false);
    let rs_outside = route_terminal_path(Path::new("/tmp/main.rs"), &facts(10), false);

    assert_eq!(
        md,
        TerminalPathRoute::PreviewTab,
        "项目内 md 应走预览标签而非源码视图"
    );
    assert_eq!(png, TerminalPathRoute::PreviewTab, "项目外图片应走预览标签");
    assert_eq!(
        rs,
        TerminalPathRoute::Workbench,
        "项目内小代码文件应走文件工作台"
    );
    assert_eq!(
        txt_outside,
        TerminalPathRoute::PreviewTab,
        "项目外 txt 受预览支持，应走预览标签"
    );
    assert_eq!(
        rs_outside,
        TerminalPathRoute::System,
        "项目外代码文件工作台无法承载，应交系统应用"
    );
}

/// 目录分流：项目内目录在工作台文件树中定位；项目外目录交系统应用（Finder）。
///
/// 数据构造：
///   is_dir=true 模拟目录（size=0、binary=false）；/p/src 在项目内，/tmp/out 在项目外
///
/// 执行过程：分别以 in_project=true / false 调用纯函数 `route_terminal_path`。
///
/// 预期结果：
///   - 项目内目录 → Directory（工作台文件树展开并选中）。
///   - 项目外目录 → System（工作台无法承载项目外目录）。
///   - 负断言：项目内目录不得再交给 System。
#[test]
fn test_route_terminal_path_reveals_project_directory_and_sends_outside_to_system() {
    let dir_facts = PathFacts {
        is_dir: true,
        size: 0,
        binary: false,
    };
    let inside = route_terminal_path(Path::new("/p/src"), &dir_facts, true);
    let outside = route_terminal_path(Path::new("/tmp/out"), &dir_facts, false);

    assert_eq!(
        inside,
        TerminalPathRoute::Directory,
        "项目内目录应在文件树中定位"
    );
    assert_ne!(
        inside,
        TerminalPathRoute::System,
        "项目内目录不得交给系统应用"
    );
    assert_eq!(outside, TerminalPathRoute::System, "项目外目录应交系统应用");
}

/// 边界：恰好 1 MiB 仍进工作台，多 1 字节、二进制、视频与 pdf 不进工作台。
///
/// 数据构造：
///   MAX = 1_048_576；等于 MAX → 允许；MAX + 1 = 1_048_577 → 超限
///   binary=true 模拟前 8 KiB 含 NUL；movie.mp4 / doc.pdf 属 Video/Other 扩展名
///
/// 预期结果：只有 MAX 本身为 Workbench，其余均为 System。
#[test]
fn test_route_terminal_path_sends_oversized_binary_and_media_to_system() {
    let exact = route_terminal_path(Path::new("/p/a.rs"), &facts(1_048_576), true);
    let over = route_terminal_path(Path::new("/p/a.rs"), &facts(1_048_577), true);
    let binary = route_terminal_path(
        Path::new("/p/a.rs"),
        &PathFacts {
            is_dir: false,
            size: 10,
            binary: true,
        },
        true,
    );
    let video = route_terminal_path(Path::new("/p/movie.mp4"), &facts(10), true);
    let pdf = route_terminal_path(Path::new("/p/doc.pdf"), &facts(10), true);

    assert_eq!(
        exact,
        TerminalPathRoute::Workbench,
        "恰好等于上限的文件应可在工作台打开"
    );
    assert_eq!(
        over,
        TerminalPathRoute::System,
        "超过 1 MiB 的文件工作台会报错，应交系统应用"
    );
    assert_eq!(
        binary,
        TerminalPathRoute::System,
        "二进制内容不能作为文本打开"
    );
    assert_eq!(video, TerminalPathRoute::System, "视频应交系统应用");
    assert_eq!(pdf, TerminalPathRoute::System, "pdf 应交系统应用");
}

/// 文件探测：真实临时文件的大小、NUL 字节嗅探和目录判定。
///
/// 数据构造：
///   text.rs 内容 "fn main() {}\n" = 13 bytes；bin.rs 内容 [b'a', 0, b'b'] = 3 bytes（含 NUL）；一个目录。
///
/// 预期结果：text.rs size=13 且非二进制；bin.rs binary=true；目录 is_dir=true。
#[test]
fn test_inspect_path_reports_size_binary_and_directory() {
    let dir = tempfile::tempdir().expect("临时目录应可创建");
    let text = dir.path().join("text.rs");
    let bin = dir.path().join("bin.rs");
    std::fs::write(&text, "fn main() {}\n").expect("写入文本夹具");
    std::fs::write(&bin, [b'a', 0, b'b']).expect("写入二进制夹具");

    let text_facts = inspect_path(&text).expect("文本文件应可探测");
    let bin_facts = inspect_path(&bin).expect("二进制文件应可探测");
    let dir_facts = inspect_path(dir.path()).expect("目录应可探测");

    assert_eq!(text_facts.size, 13, "文本文件大小应为 13 字节");
    assert!(!text_facts.binary, "纯文本不应判为二进制");
    assert!(bin_facts.binary, "含 NUL 的文件应判为二进制");
    assert!(dir_facts.is_dir, "目录应被识别");
    assert!(
        inspect_path(&dir.path().join("missing.rs")).is_err(),
        "不存在的文件应返回错误"
    );
}

/// 项目定位：来源区域优先；否则最深的包含根；根外文件无归属。
///
/// 数据构造：
///   outer = /w（area-a），inner = /w/pkg（area-b，嵌套在 outer 内）
///   文件 /w/pkg/src/x.rs 同时被两个根包含：outer 下相对路径 pkg/src/x.rs，inner 下 src/x.rs
///   文件 /elsewhere/y.rs 不在任何根内
///
/// 执行过程：
///   1. 无来源区域 → 取路径组件更多的 inner（/w/pkg 共 3 个组件，含根 `/`；/w 共 2 个）
///   2. 来源区域 = area-a → 尊重来源，取 outer
///   3. 根外文件、项目根目录本身 → None
///
/// 预期结果：见断言。
#[test]
fn test_locate_in_areas_prefers_source_area_then_deepest_root() {
    let areas = vec![
        ("area-a".to_string(), std::path::PathBuf::from("/w")),
        ("area-b".to_string(), std::path::PathBuf::from("/w/pkg")),
    ];
    let file = Path::new("/w/pkg/src/x.rs");

    let deepest = locate_in_areas(file, &areas, None);
    let preferred = locate_in_areas(file, &areas, Some("area-a"));
    let outside = locate_in_areas(Path::new("/elsewhere/y.rs"), &areas, Some("area-a"));
    let root_itself = locate_in_areas(Path::new("/w"), &areas, None);

    assert_eq!(
        deepest,
        Some(("area-b".to_string(), "src/x.rs".to_string())),
        "无来源区域时应归属最深的嵌套项目"
    );
    assert_eq!(
        preferred,
        Some(("area-a".to_string(), "pkg/src/x.rs".to_string())),
        "来源区域包含该文件时应优先使用，保持与 Agent 所在项目一致"
    );
    assert_eq!(outside, None, "不在任何项目根内的文件不应归属任何区域");
    assert_eq!(
        root_itself, None,
        "项目根目录本身不是文件，不应产生相对路径"
    );
}

/// 点击终端里的路径不能直接运行程序：可启动目标只在 Finder 中显示，其余交给默认应用。
///
/// 数据构造（权限位推导）：
///   0o644 = rw-r--r--，无任何可执行位（0o644 & 0o111 = 0）→ 普通文件
///   0o755 = rwxr-xr-x，（0o755 & 0o111 = 0o111 ≠ 0）→ 带可执行位
///   扩展名大小写混用的 "Run.COMMAND" 用来确认比较不区分大小写
///
/// 执行过程：对每个组合调用纯函数 `system_open_mode(path, is_dir, unix_mode)`。
///
/// 预期结果：
///   - /tmp/x.command（0o644）→ RevealInFinder：macOS 的 open 会在终端里执行 .command。
///   - /Applications/Foo.app（目录）→ RevealInFinder：open 会启动应用。
///   - /tmp/Run.COMMAND → RevealInFinder：扩展名大小写不影响判断。
///   - /tmp/installer.pkg、/tmp/tool.sh（0o644）→ RevealInFinder：安装包与脚本。
///   - /tmp/blob（0o755，无扩展名）→ RevealInFinder：带可执行位的文件会被 open 运行。
///   - /tmp/photo.png、/tmp/notes.txt（0o644）→ Open：普通文档照常用默认应用打开。
///   - /tmp/data（目录，0o755）→ Open：目录天然带 x 位，不能因此误判为可执行。
///   - /tmp/notes.txt 取不到权限位（None）→ Open：缺少信息时按扩展名判断。
#[test]
fn test_system_open_mode_reveals_launchable_targets_instead_of_running_them() {
    let mode = |name: &str, is_dir: bool, unix_mode: Option<u32>| {
        system_open_mode(Path::new(name), is_dir, unix_mode)
    };

    assert_eq!(
        mode("/tmp/x.command", false, Some(0o644)),
        SystemOpen::RevealInFinder,
        ".command 会被 open 在终端中执行"
    );
    assert_eq!(
        mode("/Applications/Foo.app", true, Some(0o755)),
        SystemOpen::RevealInFinder,
        ".app 目录会被 open 启动"
    );
    assert_eq!(
        mode("/tmp/Run.COMMAND", false, Some(0o644)),
        SystemOpen::RevealInFinder,
        "扩展名比较应不区分大小写"
    );
    assert_eq!(
        mode("/tmp/installer.pkg", false, Some(0o644)),
        SystemOpen::RevealInFinder,
        "安装包不应被点击直接打开"
    );
    assert_eq!(
        mode("/tmp/tool.sh", false, Some(0o644)),
        SystemOpen::RevealInFinder,
        "脚本不应被点击直接打开"
    );
    assert_eq!(
        mode("/tmp/blob", false, Some(0o755)),
        SystemOpen::RevealInFinder,
        "带可执行位的无扩展名文件会被 open 运行"
    );
    assert_eq!(
        mode("/tmp/photo.png", false, Some(0o644)),
        SystemOpen::Open,
        "普通图片应照常用默认应用打开"
    );
    assert_eq!(
        mode("/tmp/notes.txt", false, Some(0o644)),
        SystemOpen::Open,
        "普通文本应照常用默认应用打开"
    );
    assert_eq!(
        mode("/tmp/data", true, Some(0o755)),
        SystemOpen::Open,
        "普通目录自带 x 位，不能误判为可执行"
    );
    assert_eq!(
        mode("/tmp/notes.txt", false, None),
        SystemOpen::Open,
        "取不到权限位时按扩展名判断，普通文本照常打开"
    );
}

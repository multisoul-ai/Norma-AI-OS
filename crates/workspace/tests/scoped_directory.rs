#![cfg(windows)]
use std::{io::ErrorKind, process::Command};
use workspace::scoped_directory::Directory;

/// 场景：真实 NTFS 目录与文件发布；数据为独立目录和两个合法指针正文。
/// 执行：创建、刷目录、写文件、替换并刷目录，再读取。
/// 预期：目录同步真实成功；原子替换保留新正文，不得吞掉同步或权限错误。
#[test]
fn native_directory_flush_and_relative_rename_are_real() {
    let temp = tempfile::tempdir().expect("隔离 NTFS 根");
    let root = Directory::open(temp.path(), true).expect("写目录能力");
    let dir = root.create_child("state").expect("原子创建目录");
    let flush = root.sync();
    println!("native parent directory flush: {flush:?}");
    flush.expect("必须实际支持目录同步，不能将 unsupported 当作通过");
    dir.write_new("CURRENT", b"old").expect("旧指针");
    dir.write_new("next", b"new").expect("新指针");
    dir.rename("next", "CURRENT").expect("句柄相对原子替换");
    dir.sync().expect("发布后目录同步");
    assert_eq!(
        dir.read("CURRENT", 3).expect("读取"),
        b"new",
        "应读取完整新指针"
    );
    assert_eq!(
        dir.read("next", 3).expect_err("源名必须消失").kind(),
        ErrorKind::NotFound,
        "重命名不得复制源名"
    );
    assert_eq!(
        root.create_child("state")
            .err()
            .expect("禁止覆盖目录")
            .kind(),
        ErrorKind::AlreadyExists,
        "创建冲突必须保留真实 AlreadyExists"
    );
}

/// 场景：只读授权根遇到 root/祖先替换；数据为临时两层目录。
/// 执行：持有目录能力时尝试重命名根和父目录，再释放能力重试。
/// 预期：持有时两种替换都被 OS 拒绝；释放后重命名成功，不靠路径复查掩盖竞态。
#[test]
fn read_only_root_pins_root_and_every_ancestor() {
    let temp = tempfile::tempdir().expect("隔离根");
    let parent = temp.path().join("parent");
    let root = parent.join("project");
    std::fs::create_dir_all(&root).expect("两层目录");
    let directory = Directory::open(&root, false).expect("只读根句柄");
    assert!(
        std::fs::rename(&root, parent.join("moved")).is_err(),
        "已授权根不能被替换"
    );
    assert!(
        std::fs::rename(&parent, temp.path().join("moved-parent")).is_err(),
        "祖先不能被替换来重定向 Git cwd"
    );
    assert_eq!(
        directory.path().expect("句柄实际路径"),
        root.canonicalize().expect("实际根"),
        "必须保持同一根"
    );
    drop(directory);
    std::fs::rename(&root, parent.join("moved")).expect("能力释放后应可正常重命名");
}

/// 场景：Windows 别名与 junction 越界；数据为外部 sentinel、真实目录链接和危险名称。
/// 执行：通过目录能力读取别名或进入链接。
/// 预期：全部拒绝；外部 sentinel 保持原样，不新增 ignored 或静默跳过。
#[test]
fn windows_aliases_and_junction_never_escape_scope() {
    let temp = tempfile::tempdir().expect("隔离根");
    let outside = tempfile::tempdir().expect("外部目录");
    std::fs::write(outside.path().join("secret.txt"), b"private").expect("外部 sentinel");
    let output = Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(temp.path().join("escape"))
        .arg(outside.path())
        .output()
        .expect("原生 junction 创建");
    assert!(
        output.status.success(),
        "runner 必须真正构造 junction: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = Directory::open(temp.path(), true).expect("受管根");
    assert!(
        root.child("escape", false).is_err(),
        "中间 reparse point 不得被跟随"
    );
    for name in [
        "../secret",
        "..",
        "secret:stream",
        "NUL",
        "COM1.txt",
        "LPT¹",
        "file.",
        "file ",
        "a\\b",
    ] {
        assert!(
            root.open_entry(name).is_err(),
            "危险 Windows 名称必须拒绝: {name}"
        );
    }
    assert_eq!(
        std::fs::read(outside.path().join("secret.txt")).expect("外部正文"),
        b"private",
        "拒绝不能触碰外部目录"
    );
}

/// 场景：读取活动指针时另一个线程发布新版本；数据为 old/new 两个完整指针。
/// 执行：保持旧 CURRENT 文件句柄打开，在同一目录替换 CURRENT，然后分别读取旧/新句柄。
/// 预期：发布不被普通读者阻塞，旧句柄仍读旧内容，新打开的句柄只读新内容。
#[test]
fn open_pointer_reader_does_not_block_atomic_publication() {
    use std::io::Read;
    let temp = tempfile::tempdir().expect("隔离根");
    let root = Directory::open(temp.path(), true).expect("目录能力");
    root.write_new("CURRENT", b"old").expect("旧指针");
    let mut old = root.open_entry("CURRENT").expect("保持读句柄");
    root.write_new("next", b"new").expect("候选指针");
    root.rename("next", "CURRENT")
        .expect("普通读者不能阻塞原子发布");
    let mut previous = Vec::new();
    old.read_to_end(&mut previous).expect("旧句柄内容");
    assert_eq!(previous, b"old", "打开的句柄必须保持原文件身份");
    assert_eq!(
        root.read("CURRENT", 3).expect("新指针"),
        b"new",
        "新读取必须见到完整新指针"
    );
}

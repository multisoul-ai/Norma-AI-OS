use super::file_identity;
use std::fs::{self, File, OpenOptions};
use std::io::Write;

/// 场景：同一打开文件被追加；构造 old 和一次 append；
/// 执行两次句柄身份读取；预期身份稳定，不把长度变化当成文件替换。
#[test]
fn file_identity_survives_append() {
    let root = tempfile::tempdir().expect("创建身份夹具");
    let path = root.path().join("transcript.jsonl");
    fs::write(&path, b"old\n").expect("写入初始记录");
    let file = File::open(&path).expect("打开原文件句柄");
    let before = file_identity(&file).expect("查询初始身份");
    OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("打开追加句柄")
        .write_all(b"new\n")
        .expect("追加完整记录");
    assert_eq!(
        file_identity(&file).expect("查询追加后的身份"),
        before,
        "追加不能改变同一文件身份"
    );
    assert_eq!(
        file.metadata().expect("查询长度").len(),
        8,
        "old/new 加两个换行共八字节"
    );
}

/// 场景：原路径被同长度新文件替代；构造两个四字节文件；
/// 执行重命名和新建后比较打开句柄；预期旧句柄保持原身份，新文件身份不同。
#[test]
fn replacement_is_distinct_even_with_equal_length() {
    let root = tempfile::tempdir().expect("创建替换身份夹具");
    let path = root.path().join("transcript.jsonl");
    fs::write(&path, b"old\n").expect("创建旧记录");
    let old = File::open(&path).expect("保持旧文件句柄");
    let original = file_identity(&old).expect("查询旧文件身份");
    fs::rename(&path, root.path().join("archived.jsonl")).expect("移动原文件");
    fs::write(&path, b"new\n").expect("原位置写入同长度新记录");
    let new = File::open(&path).expect("打开替换文件");
    assert_eq!(
        file_identity(&old).expect("查询旧句柄"),
        original,
        "路径变化不能改变旧句柄身份"
    );
    assert_ne!(
        file_identity(&new).expect("查询新句柄"),
        original,
        "替换文件不能复用旧游标身份"
    );
    assert_eq!(
        old.metadata().expect("旧长度").len(),
        new.metadata().expect("新长度").len(),
        "反例必须保持相同长度"
    );
}

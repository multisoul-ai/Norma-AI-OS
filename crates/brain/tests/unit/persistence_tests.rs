use super::*;

/// 保存状态必须原子替换目标，不能跟随被替换成符号链接的状态文件。
/// 数据构造：指向 victim.json 的 state.json 链接，victim 保留一段已知内容。
/// 执行过程：通过保存接口写入新状态，再读取状态和原目标。
/// 预期结果：state.json 是新的普通文件，victim 内容保持不变。
#[test]
fn atomic_state_replacement_does_not_follow_symlinks() {
    let dir = tempfile::tempdir().expect("应创建夹具目录");
    let victim = dir.path().join("victim.json");
    let state = dir.path().join("state.json");
    std::fs::write(&victim, "original").expect("应创建原始文件");
    std::os::unix::fs::symlink(&victim, &state).expect("应创建符号链接");
    atomic_write(&state, b"new state").expect("应保存新状态");
    assert_eq!(
        std::fs::read_to_string(&victim).unwrap(),
        "original",
        "原目标不能被覆盖"
    );
    assert_eq!(
        std::fs::read_to_string(&state).unwrap(),
        "new state",
        "应能读回新状态"
    );
    assert!(
        !std::fs::symlink_metadata(&state)
            .unwrap()
            .file_type()
            .is_symlink(),
        "状态应为普通文件"
    );
}

/// 连续保存多个快照时，最后的快照必须保留，读者只能看到完整 JSON。
/// 数据构造：同一个 writer 连续提交 50 个版本，最后版本为 49。
/// 执行过程：等待最后一次保存完成，再解析磁盘 JSON。
/// 预期结果：版本为 49，目录没有遗留的临时文件。
#[test]
fn queued_snapshots_finish_in_submission_order() {
    let dir = tempfile::tempdir().expect("应创建夹具目录");
    let path = dir.path().join("state.json");
    let writer = Writer::default();
    let mut last = None;
    for version in 0..50 {
        last = Some(writer.save(path.clone(), format!("{{\"version\":{version}}}")));
    }
    last.unwrap()
        .recv()
        .expect("后台线程应返回结果")
        .expect("应保存全部快照");
    let read: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).expect("状态必须是完整 JSON");
    assert_eq!(read["version"], 49, "最后一次提交应最终落盘");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "不得残留临时文件"
    );
}

/// 旧拥有者仍有积压写入时，重开的拥有者必须等旧队列完成才能读取。
/// 数据构造：FIFO 暂停全局 writer，旧拥有者排队保存新快照，新拥有者提交读取屏障。
/// 执行过程：确认屏障尚未完成，打开 FIFO 解除 IO 阻塞，再等待屏障并读盘。
/// 预期结果：重开后读到已移除提议的快照，不能读到之前的待办。
#[test]
fn reopened_owner_waits_for_previously_queued_snapshots() {
    use std::io::Read;
    let dir = tempfile::tempdir().expect("应创建夹具目录");
    let fifo = dir.path().join("pause");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success(),
        "应创建可控 IO 夹具"
    );
    let state = dir.path().join("state.json");
    atomic_write(&state, b"pending").expect("应保存旧提议");
    let old_owner = shared_writer().clone();
    let paused = old_owner.append(fifo.clone(), "resume".into());
    old_owner.save(state.clone(), "removed".into());
    drop(old_owner);
    let new_owner = shared_writer().clone();
    let barrier = new_owner.flush();
    assert!(
        matches!(
            barrier.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ),
        "不能越过积压的旧写入"
    );
    let mut content = String::new();
    std::fs::File::open(&fifo)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    paused.recv().unwrap().expect("应解除暂停");
    barrier.recv().unwrap().expect("旧写入应全部完成");
    assert_eq!(
        std::fs::read_to_string(state).unwrap(),
        "removed",
        "不能恢复已处理的提议"
    );
}

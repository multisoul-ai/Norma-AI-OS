use super::*;
use std::{
    fs::{self, File, FileTimes},
    os::unix::fs::symlink,
    sync::mpsc,
    thread,
    time::Duration,
};

/// LOG-1：日志目录必须位于用户主目录下的 `.norma/logs`。
///
/// 数据构造（含关键数值的推导过程）：
///   home dir = /tmp/norma-user
///   log path segments added = 2（固定目录 `.norma` + 固定子目录 `logs`）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将用户主目录传入 log_directory
///   2. 在主目录后依次追加 `.norma` 和 `logs`
///
/// 预期结果：
///   - 正断言：结果等于 home/.norma/logs
///   - 负断言：结果不等于旧的 home/Library/Application Support/Norma OS/logs
#[test]
fn test_log_directory_is_scoped_below_user_home() {
    let home_dir = Path::new("/tmp/norma-user");
    let old_app_data_log_directory = home_dir
        .join("Library")
        .join("Application Support")
        .join("Norma OS")
        .join("logs");

    let actual = log_directory(home_dir);

    assert_eq!(
        actual,
        home_dir.join(".norma").join("logs"),
        "日志目录应固定放在用户主目录的 .norma/logs 子目录"
    );
    assert_ne!(
        actual, old_app_data_log_directory,
        "日志不得继续写入旧的 Application Support/Norma OS/logs 目录"
    );
}

/// LOG-2：清理只删除超过保留期的 Norma OS 日志，保留近期、外部和非文件条目。
///
/// 数据构造（含关键数值的推导过程）：
///   retention = 14 days = 14 * 24 * 60 * 60 seconds
///   old log age = retention + 1 second（已经过期）
///   recent log age = retention - 1 second（仍在保留期内）
///   matching regular logs = 2（old + recent）
///   foreign regular logs = 1（文件名不属于 Norma OS logger）
///   matching non-files = 2（一个目录 + 一个指向 foreign 的符号链接）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 扫描受控日志目录中的五个条目
///   2. 按文件名筛出 Norma OS logger 自己创建的普通文件
///   3. 比较修改时间与 14 天保留期
///   4. 删除 old，保留 recent、foreign、同名目录与符号链接
///
/// 预期结果：
///   - 正断言：恰好删除一个过期日志且无清理失败
///   - 正断言：recent、foreign、同名目录和符号链接都继续存在
///   - 负断言：old 日志不应继续存在
#[test]
fn test_cleanup_removes_only_expired_owned_log_files() {
    let temp = tempfile::tempdir().expect("test should create temp log directory");
    let log_dir = temp.path();
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000);
    // retention = 14 days * 24 hours * 60 minutes * 60 seconds.
    let retention = Duration::from_secs(14 * 24 * 60 * 60);
    let old_log = log_dir.join("norma-os.2026-07-01.log");
    let recent_log = log_dir.join("norma-os.2026-07-20.log");
    let foreign_log = log_dir.join("unrelated.2026-07-01.log");
    let matching_directory = log_dir.join("norma-os.2026-06-01.log");
    let matching_symlink = log_dir.join("norma-os.2026-05-01.log");

    fs::write(&old_log, b"old").expect("test should write old log");
    fs::write(&recent_log, b"recent").expect("test should write recent log");
    fs::write(&foreign_log, b"foreign").expect("test should write foreign log");
    fs::create_dir(&matching_directory).expect("test should create matching directory");
    symlink(&foreign_log, &matching_symlink).expect("test should create matching symlink");

    // old age = retention + 1 second, so it must be removed.
    set_modified(&old_log, now - retention - Duration::from_secs(1));
    // recent age = retention - 1 second, so it must be retained.
    set_modified(&recent_log, now - retention + Duration::from_secs(1));
    // A foreign file remains protected even when it is old.
    set_modified(&foreign_log, now - retention - Duration::from_secs(1));

    let report =
        cleanup_expired_logs(log_dir, now, retention).expect("log cleanup should scan temp dir");

    assert_eq!(
        report.removed, 1,
        "清理应恰好删除一个超过 14 天的受控日志文件"
    );
    assert_eq!(report.failed, 0, "所有可访问测试文件都应成功完成清理判断");
    assert!(
        !old_log.exists(),
        "超过保留期的 norma-os 日志不应在清理后继续存在"
    );
    assert!(
        recent_log.exists(),
        "仍在保留期内的 norma-os 日志必须继续存在"
    );
    assert!(
        foreign_log.exists(),
        "文件名不属于 Norma OS logger 的文件不得被清理"
    );
    assert!(
        matching_directory.exists(),
        "即使名称匹配，非普通文件条目也不得被日志清理器删除"
    );
    assert!(
        matching_symlink.symlink_metadata().is_ok(),
        "即使名称匹配，日志清理器也不得跟随或删除符号链接"
    );
}

/// LOG-3：生产 rolling appender 必须把 tracing 事件持久化为无 ANSI 的按日日志文件。
///
/// 数据构造（含关键数值的推导过程）：
///   emitted events = 1（唯一 marker 为 `disk-persistence-marker`）
///   expected daily files = 1（空目录首次写入只创建当天文件）
///   ANSI escape bytes = 0（磁盘日志应为纯文本）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在临时目录创建生产配置使用的 daily rolling appender
///   2. 用 non-blocking writer 安装局部 subscriber 并发送一个 info 事件
///   3. drop WorkerGuard，等待缓冲内容刷新到磁盘
///   4. 读取唯一日志文件的名称和正文
///
/// 预期结果：
///   - 正断言：只创建一个归属于 Norma OS logger 的文件
///   - 正断言：文件正文包含唯一 marker
///   - 负断言：正文不包含 ANSI escape 字节
#[test]
fn test_rolling_appender_persists_plain_text_event_to_disk() {
    let temp = tempfile::tempdir().expect("test should create temp log directory");
    let appender =
        build_file_appender(temp.path()).expect("production rolling appender should initialize");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(writer)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("disk-persistence-marker");
    });
    drop(guard);

    let files = fs::read_dir(temp.path())
        .expect("test should scan rolling log directory")
        .map(|entry| {
            entry
                .expect("test log directory entry should be readable")
                .path()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        files.len(),
        1,
        "首次写入一个事件应只创建一个当天 rolling 日志文件"
    );
    let file_name = files[0]
        .file_name()
        .expect("rolling log should have a file name");
    let contents = fs::read_to_string(&files[0]).expect("rolling log should be valid UTF-8");

    assert!(
        is_owned_log_filename(file_name),
        "rolling appender 创建的文件名必须能被受控日志清理器识别"
    );
    assert!(
        contents.contains("disk-persistence-marker"),
        "WorkerGuard drop 后日志文件必须包含已发送的 tracing 事件"
    );
    assert!(
        !contents.contains('\u{1b}'),
        "磁盘日志不得包含终端 ANSI escape 字节"
    );
}

/// LOG-4：维护循环必须在启动时和后续周期持续清理新出现的过期日志。
///
/// 数据构造（含关键数值的推导过程）：
///   retention = 1 second
///   cleanup interval = 10 milliseconds
///   expired age = retention + 1 second = 2 seconds
///   wait budget = 200 attempts * 10 milliseconds = 2 seconds
///   expired files = 2（startup-old + periodic-old）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动前创建 startup-old，维护线程首轮立即删除
///   2. 首轮完成后创建 periodic-old
///   3. 等待至少一个 10ms 周期，维护线程再次扫描并删除
///   4. 发送 shutdown 并 join 线程
///
/// 预期结果：
///   - 正断言：startup-old 被首轮清理
///   - 正断言：periodic-old 被后续周期清理
///   - 负断言：维护线程收到 shutdown 后不应继续存活
#[test]
fn test_maintenance_loop_cleans_at_startup_and_on_later_interval() {
    let temp = tempfile::tempdir().expect("test should create temp log directory");
    let now = SystemTime::now();
    // retention = 1 second; expired age = 1 + 1 = 2 seconds.
    let retention = Duration::from_secs(1);
    // cleanup interval = 10 milliseconds; max wait = 200 * 10ms = 2 seconds.
    let cleanup_interval = Duration::from_millis(10);
    let startup_old = temp.path().join("norma-os.2026-07-01.log");
    fs::write(&startup_old, b"startup").expect("test should write startup-old log");
    set_modified(&startup_old, now - retention - Duration::from_secs(1));

    let (shutdown, receiver) = mpsc::channel();
    let log_directory = temp.path().to_path_buf();
    let worker = thread::spawn(move || {
        maintain_logs_until_shutdown(&log_directory, retention, cleanup_interval, receiver);
    });

    wait_until_removed(&startup_old);
    let periodic_old = temp.path().join("norma-os.2026-07-02.log");
    fs::write(&periodic_old, b"periodic").expect("test should write periodic-old log");
    set_modified(
        &periodic_old,
        SystemTime::now() - retention - Duration::from_secs(1),
    );
    wait_until_removed(&periodic_old);

    shutdown
        .send(())
        .expect("maintenance test should send shutdown");
    // shutdown wait budget = 200 attempts * 10 milliseconds = 2 seconds.
    for _ in 0..200 {
        if worker.is_finished() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        worker.is_finished(),
        "维护线程收到 shutdown 后必须退出而不是继续后台存活"
    );
    worker
        .join()
        .expect("maintenance thread should stop after shutdown");

    assert!(
        !startup_old.exists(),
        "维护循环启动时必须删除已经过期的受控日志"
    );
    assert!(
        !periodic_old.exists(),
        "维护循环后续周期必须删除运行期间新出现的过期日志"
    );
}

fn set_modified(path: &Path, modified: SystemTime) {
    File::options()
        .write(true)
        .open(path)
        .expect("test should open file for timestamp update")
        .set_times(FileTimes::new().set_modified(modified))
        .expect("test should set file modification time");
}

fn wait_until_removed(path: &Path) {
    // wait budget = 200 attempts * 10 milliseconds = 2 seconds.
    for _ in 0..200 {
        if !path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "timed out waiting for maintenance to remove {}",
        path.display()
    );
}

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// 场景：后台打开数据库时，另一个连接正持有写锁，壁纸迁移必须等待而非读锁升级失败。
/// 数据构造：完整旧表但无壁纸迁移标记；主连接 BEGIN IMMEDIATE，后台尝试迁移。
/// 执行过程：确认后台已返回错误或实际进入写锁等待，再释放主连接并收取迁移结果。
/// 预期结果：迁移成功且标记只写一次，已存在的独立壁纸值不变。
#[test]
fn wallpaper_migration_waits_for_writer_before_reading() {
    static WAITING: AtomicBool = AtomicBool::new(false);
    WAITING.store(false, Ordering::SeqCst);
    let directory = tempfile::tempdir().expect("隔离迁移目录");
    let path = directory.path().join("wallpaper.sqlite");
    let owner = Connection::open(&path).expect("持锁连接");
    owner
        .execute_batch(
            "CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT,updated_at_unix INTEGER);
        CREATE TABLE desktop_workspaces(id TEXT,wallpaper_json TEXT);
        CREATE TABLE desktop_project_areas(id TEXT,wallpaper_json TEXT);
        INSERT INTO desktop_workspaces VALUES('space','independent');
        BEGIN IMMEDIATE;",
        )
        .expect("旧表与写锁");
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let connection = Connection::open(path).expect("后台连接");
        connection
            .busy_handler(Some(|attempt| {
                WAITING.store(true, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(1));
                attempt < 2_000
            }))
            .expect("观察真实写锁争用");
        sender
            .send(migrate_wallpapers(&connection))
            .expect("回传迁移结果");
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut early = None;
    while !WAITING.load(Ordering::SeqCst) && Instant::now() < deadline {
        if let Ok(result) = receiver.try_recv() {
            early = Some(result);
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let waited = WAITING.load(Ordering::SeqCst);
    owner.execute_batch("COMMIT").expect("释放主连接写锁");
    let result = early.unwrap_or_else(|| {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("后台迁移应完成")
    });
    worker.join().expect("后台线程");
    assert!(
        result.is_ok(),
        "迁移不得因 Deferred 读锁升级返回 Busy：{result:?}"
    );
    assert!(waited, "必须实际观察到写锁争用，不能靠后台迟调度通过");
    let markers: i64 = owner
        .query_row(
            "SELECT count(*) FROM settings WHERE key='wallpaper.desktop_migrated.v1'",
            [],
            |row| row.get(0),
        )
        .expect("迁移标记");
    assert_eq!(markers, 1, "成功迁移必须且只能保存一次标记");
    let wallpaper: String = owner
        .query_row(
            "SELECT wallpaper_json FROM desktop_workspaces WHERE id='space'",
            [],
            |row| row.get(0),
        )
        .expect("原壁纸");
    assert_eq!(wallpaper, "independent", "迁移不得覆盖独立壁纸");
}

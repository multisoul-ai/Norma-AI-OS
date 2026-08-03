use super::*;
use crate::test_backend::TestPtyBackend;
use crossbeam_channel::unbounded;

/// 注入式 e2e：spawn_with_backend + TestPtyBackend::echo(tx) 驱动 write→drain→snapshot，
/// 验证 backend 注入路径闭环且不启动真实子进程。
///
/// 数据构造（含关键数值推导）：
///   通道：unbounded() → (tx, rx) 一对
///   backend = TestPtyBackend::echo(tx)：write(bytes) 经 tx.send(Output(bytes)) 同步回吐
///   session = spawn_with_backend(7, 80, 24, Box::new(backend), rx)
///     —— backend 持 tx（write 走 tx），session 持 rx（drain 走 rx），共享通道闭环
///   写入：write_bytes(b"hi") = 2 字节
///     echo 模式 → emit(b"hi") → tx.send(Output(b"hi")) 同步成功 → rx 立即可取
///
/// 执行过程：
///   1. 建通道 (tx, rx)
///   2. echo(tx) 构造 backend，tx 所有权移入 backend
///   3. spawn_with_backend(7, 80, 24, Box::new(backend), rx) 注入 session，rx 移入 session
///   4. write_bytes(b"hi") → backend.write → emit → tx.send(Output) → 通道收到事件
///   5. drain() 从 rx 取 Output(b"hi") → grid.advance → dirty=true，应返回 true
///   6. 再次 drain() 通道已空 → 返回 false
///   7. snapshot() 取 grid 当前屏幕
///
/// 预期结果：
///   - 正断言：首次 drain 返回 true（注入后端确实回吐了事件到通道）
///   - 正断言：快照首行包含 "hi"（echo 回吐的字节经 grid.advance 落盘）
///   - 正断言：session.id == 7（注入路径保留 id）
///   - 负断言：第二次 drain 返回 false（通道无遗留，无幽灵事件）
///   - 负断言：快照首行不应包含未写入的 "bye"
#[test]
fn test_spawn_with_backend_echo_drives_session() {
    let (tx, rx) = unbounded();
    let backend = TestPtyBackend::echo(tx);
    let mut session = TerminalSession::spawn_with_backend(7, 80, 24, Box::new(backend), rx);

    session
        .write_bytes(b"hi")
        .expect("write_bytes 经注入后端应成功");

    let dirty_first = session.drain();
    let dirty_second = session.drain();
    let snapshot = session.snapshot();
    let first_row: String = snapshot.rows[0].iter().map(|cell| cell.c).collect();

    assert!(
        dirty_first,
        "注入后端 write 后应首次 drain 返回 true，否则说明通道未收到 echo 回吐"
    );
    assert!(
        first_row.contains("hi"),
        "快照首行应含 echo 回吐的 'hi'，实际: {:?}",
        first_row
    );
    assert_eq!(session.id, 7, "注入构造应保留 session_id=7");
    assert!(
        !dirty_second,
        "通道应已耗尽，第二次 drain 不应再返回 true（无幽灵事件）"
    );
    assert!(
        !first_row.contains("bye"),
        "快照首行不应含未写入的 'bye'，实际: {:?}",
        first_row
    );
}

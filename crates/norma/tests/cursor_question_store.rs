use norma::cursor::store::CursorQuestionStore;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
fn field(number: u64, value: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for mut n in [number * 8 + 2, value.len() as u64] {
        while n >= 128 {
            bytes.push((n as u8 & 127) | 128);
            n >>= 7;
        }
        bytes.push(n as u8);
    }
    bytes.extend(value);
    bytes
}
fn blob(connection: &Connection, bytes: &[u8]) -> String {
    let hash = format!("{:x}", Sha256::digest(bytes));
    connection
        .execute(
            "INSERT OR REPLACE INTO blobs VALUES(?1,?2)",
            params![hash, bytes],
        )
        .unwrap();
    hash
}
fn current(connection: &Connection, session: &str, root: &[u8]) {
    let id = blob(connection, root);
    let metadata = json!({"agentId":session,"latestRootBlobId":id});
    connection
        .execute(
            "INSERT OR REPLACE INTO meta VALUES('0',?1)",
            [hex::encode(metadata.to_string())],
        )
        .unwrap();
}

/// 场景：只沿精确会话的当前 root 读取待答题和有界工具结果，不扫描旧 blobs 或兄弟会话。
/// 构造：真实录制的工具调用/结果，当前 pending 根和一个未被引用的旧结果blob；随后提升新根。
/// 执行：读取当前pending，验证无旧消费；再让原结果挂到当前root并读取；篡改meta会话进行负例。
/// 预期：只有当前root field4可登记；只在field1链接结果后确认消费；错会话拒绝，始终只读。
#[test]
fn current_cursor_root_owns_pending_question_and_linked_consumption() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let reader =
        CursorQuestionStore::for_session(root, "/tmp/original/../workspace", "original-session")
            .unwrap();
    std::fs::create_dir_all(reader.path().parent().unwrap()).unwrap();
    let db = Connection::open(reader.path()).unwrap();
    db.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT);CREATE TABLE blobs(id TEXT PRIMARY KEY,data BLOB);").unwrap();
    let call: Value =
        serde_json::from_str(include_str!("fixtures/questions/cursor-tool-call.json")).unwrap();
    let result: Value =
        serde_json::from_str(include_str!("fixtures/questions/cursor-tool-result.json")).unwrap();
    let stale = blob(
        &db,
        json!({"role":"tool","content":[result.clone()]})
            .to_string()
            .as_bytes(),
    );
    current(
        &db,
        "original-session",
        &field(
            4,
            json!({"role":"assistant","content":[call]})
                .to_string()
                .as_bytes(),
        ),
    );
    let pending = reader.read().unwrap();
    assert_eq!(pending.pending.len(), 1, "当前root原题可登记");
    assert!(
        pending.results.is_empty(),
        "未链接旧结果不能冒认当前题已完成"
    );
    current(
        &db,
        "original-session",
        &field(1, &hex::decode(stale).unwrap()),
    );
    let consumed = reader.read().unwrap();
    assert!(consumed.pending.is_empty(), "新root不再待答");
    assert_eq!(
        consumed.results,
        vec![result],
        "只返回当前root链接的工具结果"
    );
    current(&db, "foreign-session", &field(1, &[1; 32]));
    assert!(
        reader.read().is_err(),
        "路径相同也必须核对meta的原session身份"
    );
    assert!(
        !root.join("chats/foreign-session").exists(),
        "读取不能创建或遍历别的会话"
    );
}

/// 场景：Cursor 路径按 path.resolve(cwd) 的MD5定位，符号链接名字不可自行canonicalize。
/// 构造：两个词法等价cwd和一个不同别名，以及非法session与缺失数据库。
/// 执行：构造路径并读不存在的文件。
/// 预期：等价路径相同、别名不同、越界ID拒绝，读取缺失文件不会创建数据库。
#[test]
fn cursor_store_uses_exact_lexical_workspace_and_never_creates_a_database() {
    let root = tempfile::tempdir().unwrap();
    let left =
        CursorQuestionStore::for_session(root.path(), "/tmp/a/../workspace", "session").unwrap();
    let right = CursorQuestionStore::for_session(root.path(), "/tmp/workspace", "session").unwrap();
    assert_eq!(left.path(), right.path(), "词法resolve一致");
    let alias = CursorQuestionStore::for_session(root.path(), "/tmp/alias", "session").unwrap();
    assert_ne!(
        left.path(),
        alias.path(),
        "不把符号链接别名归到另一workspace"
    );
    assert!(
        CursorQuestionStore::for_session(root.path(), "/tmp/workspace", "../foreign").is_err(),
        "session不得越界"
    );
    assert!(left.read().is_err(), "缺失原store不可读");
    assert!(!left.path().exists(), "只读不能创建空store冒充会话");
}

/// 场景：损坏或越界的当前根不得产生可执行题目，旧巨型blob不影响当前根读取。
/// 构造：有效待答题、未引用的超大blob、缺失引用、坏protobuf以及内容hash不匹配。
/// 执行：逐次替换meta指向的当前根，最后篡改当前root内容。
/// 预期：有效根可读取且不扫描旧blob；每个损坏当前根均返回错误，不返回部分题目。
#[test]
fn cursor_store_fails_closed_on_corrupt_current_root_without_scanning_old_blobs() {
    let directory = tempfile::tempdir().unwrap();
    let reader =
        CursorQuestionStore::for_session(directory.path(), "/tmp/workspace", "session").unwrap();
    std::fs::create_dir_all(reader.path().parent().unwrap()).unwrap();
    let db = Connection::open(reader.path()).unwrap();
    db.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT);CREATE TABLE blobs(id TEXT PRIMARY KEY,data BLOB);").unwrap();
    db.execute(
        "INSERT INTO blobs VALUES('unreferenced',zeroblob(9000000))",
        [],
    )
    .unwrap();
    let call: Value =
        serde_json::from_str(include_str!("fixtures/questions/cursor-tool-call.json")).unwrap();
    let valid = field(
        4,
        json!({"role":"assistant","content":[call]})
            .to_string()
            .as_bytes(),
    );
    current(&db, "session", &valid);
    assert_eq!(
        reader.read().unwrap().pending.len(),
        1,
        "无关9MB旧blob不参与当前root读取"
    );
    let mut duplicate = valid.clone();
    duplicate.extend(&valid);
    for bad in [
        vec![0],
        vec![34, 255],
        vec![255; 11],
        field(1, &[1; 32]),
        duplicate,
    ] {
        current(&db, "session", &bad);
        assert!(
            reader.read().is_err(),
            "坏framing、缺引用或重复pending不得返回部分题目"
        );
    }
    current(&db, "session", &valid);
    let root_id = format!("{:x}", Sha256::digest(&valid));
    db.execute(
        "UPDATE blobs SET data=?1 WHERE id=?2",
        params![vec![0u8], root_id],
    )
    .unwrap();
    assert!(reader.read().is_err(), "内容必须匹配root SHA256");
}

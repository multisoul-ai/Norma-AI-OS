use storage::{AppStore, DatabasePaths};

/// 场景：原生平台创建中文工作区后，精确记录与身份必须在重开数据库后仍存在。
/// 构造：隔离的“中文 data”目录、初始0条记录，名称为“官网 中文工作区”。
/// 执行：通过正式AppStore入口初始化并创建，读取精确ID；释放Store后重新初始化并查询。
/// 预期：仅有一条完整中文记录、无自动名称，ID保持36字符的UUID v4格式且重开不变。
/// 数值：16字节编码为32个十六进制字符，加4个分隔符共36字符；版本位为4、variant为10。
#[test]
fn native_chinese_workspace_creation_persists_across_reopen() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("中文 data"));
    let app = AppStore::initialize(&paths).expect("应通过正式入口初始化数据库");
    let catalog = app.desktop_workspace_store();
    assert!(
        catalog.list().expect("应读取初始目录").is_empty(),
        "夹具必须从空目录开始"
    );

    let name = "官网 中文工作区";
    let created = catalog
        .create(name)
        .expect("原生平台必须能够创建中文工作区");
    assert_eq!(created.name, name, "创建结果必须保留精确中文名称");
    let id = created.id.as_bytes();
    assert_eq!(id.len(), 36, "工作区ID必须保留既有UUID格式");
    for index in [8, 13, 18, 23] {
        assert_eq!(id[index], b'-', "UUID分隔符必须位于位置{index}");
    }
    assert_eq!(id[14], b'4', "工作区ID必须保留UUID v4版本");
    assert!(
        matches!(id[19], b'8' | b'9' | b'a' | b'b'),
        "UUID variant必须保持高位10"
    );
    assert!(
        id.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(byte),
        }),
        "ID非分隔位置只能包含小写十六进制数字"
    );
    assert_eq!(
        catalog.get(&created.id).expect("应按身份查询已创建记录"),
        Some(created.clone()),
        "返回成功的创建必须有对应持久记录"
    );
    drop(catalog);
    drop(app);

    let reopened = AppStore::initialize(&paths).expect("关闭后应重新初始化同一数据库");
    let catalog = reopened.desktop_workspace_store();
    let saved = catalog.list().expect("重开后应读取工作区目录");
    assert_eq!(
        saved,
        vec![created.clone()],
        "重开必须只保留原中文工作区，不能补建默认空间"
    );
    assert_eq!(
        catalog.get(&created.id).expect("重开后应按同一身份查询"),
        Some(created),
        "关闭重开不得更换工作区身份或丢失名称"
    );
    assert!(
        catalog
            .get("missing-workspace")
            .expect("缺失查询应成功")
            .is_none(),
        "缺失身份不得误读其他工作区"
    );
}

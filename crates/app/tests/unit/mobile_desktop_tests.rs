use super::{attach, Command, MobileController};
use gpui::TestAppContext;
use std::{
    io::Read,
    net::TcpStream,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use storage::AppStore;

/// 验证码与二维码共享同一临时凭据，取消或过期后不能继续复制。
///
/// 数据构造：隔离数据库、随机TLS身份与回环服务；120s沿用生产配对有效期，
///   expires=Instant::now()即到期边界；clipboard-sentinel用于区分未发生的复制。
/// 执行过程：签发真实二维码与验证码 → 核验六位数字 → 复制 → 标记过期 → 取消配对。
/// 预期结果：验证码为六位数字且未创建授权；有效时可复制，过期时不覆盖剪贴板，取消时清除码。
#[test]
fn pairing_code_copy_preserves_pairing_and_expiry() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let identity = norma::mobile::ServerIdentity::generate().expect("测试TLS身份");
    let server = crate::mobile::MobileServer::start(
        crate::mobile::MobileAccess::with_identity_for_e2e(store.clone(), identity),
        vec!["127.0.0.1:0".parse().expect("回环地址")],
    )
    .expect("真实测试服务");
    let (image, code, expires) = super::work::code(&server).expect("生产签发入口");
    assert_eq!(code.len(), 6, "验证码必须是六位，不再复制长凭据");
    assert!(
        code.bytes().all(|c| c.is_ascii_digit()),
        "验证码只能包含 ASCII 数字"
    );
    assert!(!code.contains("norma-pair-v1:"), "不能残留长连接码格式");
    assert!(
        store
            .mobile_access_store()
            .list()
            .expect("读取授权")
            .is_empty(),
        "生成验证码不能直接授权手机"
    );
    let mut cx = TestAppContext::single();
    let owner = cx.update(|cx| attach(store, cx));
    owner.update(&mut cx, |host, cx| {
        host.install_code(image, code.clone(), expires, cx);
        host.copy_pairing_code(cx);
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(code),
            "复制应保留完整码"
        );
        assert!(host.code_copied, "复制后必须有状态反馈");
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("clipboard-sentinel".into()));
        host.expires = Some(std::time::Instant::now());
        host.copy_pairing_code(cx);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("clipboard-sentinel"),
            "过期码不能覆盖剪贴板"
        );
        host.clear_pairing();
        assert!(
            host.pairing_code.is_none(),
            "取消后不得保留可复制的临时凭据"
        );
        assert!(!host.code_copied, "取消后不能继续显示旧复制成功状态");
    });
}

/// 同一 App 数据库的窗口共用一个移动服务；关窗保留，主动退出停止连接和迟到启动。
///
/// 数据构造：两个 App 数据库、A 的两个窗口引用、一个回环 TLS 服务；读超时 1s = 20ms listener 轮询 + 调度余量。
/// 执行过程：附着 A 两次和 B 一次 → A 注入服务 → 释放窗口引用 → 核验保活 → 主动shutdown。
/// 预期结果：A 引用相同且 B 不同；全部关窗后服务保留；主动退出后弱引用/连接失效，取消标志生效。
#[test]
fn shared_desktop_owner_survives_windows_until_app_quit() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let mut cx = TestAppContext::single();
    let first = cx.update(|cx| attach(store.clone(), cx));
    let second = cx.update(|cx| attach(store.clone(), cx));
    let other = cx.update(|cx| attach(AppStore::new(dir.path().join("other.sqlite")), cx));
    assert_eq!(
        first.entity_id(),
        second.entity_id(),
        "同数据库窗口必须共用 owner"
    );
    assert_ne!(
        first.entity_id(),
        other.entity_id(),
        "另一 App 数据库不得串入服务"
    );
    assert!(
        !first.read_with(&cx, |host: &MobileController, _| host.enabled()),
        "启动 App 不应默认监听网络"
    );
    let server = Arc::new(
        crate::mobile::MobileServer::start(
            crate::mobile::MobileAccess::with_identity_for_e2e(
                store,
                norma::mobile::ServerIdentity::generate().expect("TLS"),
            ),
            vec!["127.0.0.1:0".parse().expect("回环")],
        )
        .expect("真实测试服务"),
    );
    let address = server.addresses()[0];
    first.update(&mut cx, |host, _| host.server = Some(server.clone()));
    let cancelled = first.read_with(&cx, |host, _| host.cancelled.clone());
    let weak = first.downgrade();
    drop(first);
    cx.run_until_parked();
    assert!(weak.upgrade().is_some(), "第二个窗口仍持有服务");
    let mut socket = TcpStream::connect(address).expect("一个窗口关闭后仍可连接");
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("有界等待");
    drop(second);
    cx.update(|_| {});
    cx.run_until_parked();
    assert!(weak.upgrade().is_some(), "进程必须持有无窗口手机服务");
    assert!(
        !cancelled.load(Ordering::Acquire),
        "关窗不能取消仍启用的服务"
    );
    assert!(
        server.begin_pairing("background").is_ok(),
        "无窗口仍可服务已授权手机"
    );
    cx.update(super::shutdown);
    // GPUI releases entity payloads when the next App update flushes effects.
    cx.update(|_| {});
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "主动退出后全局注册表不能继续持有服务"
    );
    assert!(
        cancelled.load(Ordering::Acquire),
        "迟到的 Keychain/网络启动结果必须被取消"
    );
    let mut byte = [0];
    assert!(
        !matches!(socket.read(&mut byte), Ok(1..)),
        "后台持有 server Arc 也不能阻止主动退出关停网络"
    );
    assert!(
        server.begin_pairing("closed").is_err(),
        "关闭后的 service 不得继续签发二维码"
    );
}

/// 取消权限编辑时刷新真实授权版本，丢弃未保存范围，不能写入手机权限。
///
/// 数据构造：一台只读手机 revision=1；编辑草稿增加 control；数据库另一次修改使 revision=1+1=2。
/// 执行过程：进入编辑 → 改草稿 → 外部更新授权 → Reload → 再次编辑。
/// 预期结果：旧草稿与编辑状态清除；新编辑使用 revision=2 的只读权限，草稿不得写回数据库。
#[test]
fn reload_cancels_permission_draft_and_uses_current_revision() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let access = storage::MobileProjectAccess {
        workspace_id: "project-a".into(),
        access: storage::MobilePermission::Read,
    };
    let grant = store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            std::slice::from_ref(&access),
        )
        .expect("初始只读授权");
    let mut cx = TestAppContext::single();
    let owner = cx.update(|cx| attach(store.clone(), cx));
    owner.update(&mut cx, |host, cx| {
        host.edit(&grant, cx);
        host.selected
            .insert("project-a".into(), storage::MobilePermission::Control);
    });
    store
        .mobile_access_store()
        .replace_permissions("phone-a", 1, &[access])
        .expect("外部授权版本推进");
    owner.update(&mut cx, |host, cx| host.request(Command::Reload, cx));
    cx.run_until_parked();
    owner.update(&mut cx, |host, cx| {
        assert!(
            host.editing.is_none(),
            "刷新必须退出旧版本编辑，不能困在冲突状态"
        );
        assert!(
            host.selected.is_empty(),
            "取消后未保存的 control 草稿不得保留"
        );
        assert!(!host.busy, "后台刷新完成后必须允许重新编辑");
        let fresh = host.devices[0].clone();
        assert_eq!(fresh.revision, 2, "必须读取最新授权版本");
        host.edit(&fresh, cx);
        assert_eq!(
            host.editing,
            Some(("phone-a".into(), 2)),
            "重新编辑需绑定新版本"
        );
        assert_eq!(
            host.selected.get("project-a"),
            Some(&storage::MobilePermission::Read),
            "新草稿只能来自已保存权限"
        );
    });
    let saved = store.mobile_access_store().list().expect("持久化授权");
    assert!(
        !saved[0].allows("project-a", true),
        "取消不能把 control 草稿写入数据库"
    );
    assert!(saved[0].allows("project-a", false), "原有只读授权仍应保留");
}

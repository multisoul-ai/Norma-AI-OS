use serde_json::json;
use std::sync::{atomic::Ordering, Arc};
#[path = "support/mobile_realtime.rs"]
mod support;
use support::*;

#[path = "support/realtime_gateway_cases.rs"]
mod gateway;

/// 同一原始语音回合重连后仍受连续三次失败预算约束。
///
/// 数据构造：三次真正前台拒绝=既有连续失败阈值；新连接第4次本会成功但不应进入前台。
/// 执行过程：首连接执行三个不同function → 重连同media/item → 发第四个function。
/// 预期结果：持久预算阻断第四次；旧连接的拒绝不会因新sideband而清零。
#[test]
fn reconnect_keeps_the_exhausted_turn_budget() {
    let fixture = Fixture::new(
        || {},
        |request| {
            let _ = request.reply.send(Err("fixture_rejected"));
        },
    );
    let listener = fixture.upstream.try_clone().expect("首连接");
    let first = std::thread::spawn(move || {
        let mut socket = attach(&listener);
        turn(&mut socket, "item-budget", "请修复登录测试");
        for index in 0..3 {
            let function = format!("call_budget_{index}");
            call(
                &mut socket,
                &format!("response_{index}"),
                &function,
                "修复登录测试",
            );
            assert_eq!(
                output(&mut socket, &function)["status"],
                "rejected",
                "前台拒绝必须有真实回执"
            );
            continuation(&mut socket);
        }
    });
    let phone = fixture.bind(1);
    first.join().expect("首连接结束");
    drop(phone);
    let listener = fixture.upstream.try_clone().expect("重连");
    let second = std::thread::spawn(move || {
        let mut socket = attach(&listener);
        turn(&mut socket, "item-budget", "请修复登录测试");
        call(
            &mut socket,
            "response-fourth",
            "call_budget_fourth",
            "修复登录测试",
        );
        let result = output(&mut socket, "call_budget_fourth");
        assert_eq!(result["status"], "rejected", "预算耗尽不能返回accepted");
        assert!(
            result["message"].as_str().unwrap_or("").contains("预算"),
            "拒绝必须来自恢复后的预算"
        );
    });
    let _phone = fixture.bind(2);
    second.join().expect("重连结束");
    assert_eq!(
        fixture.writes.load(Ordering::SeqCst),
        3,
        "第4次不能进入前台产生副作用"
    );
}

/// 同媒体会话换连接且权限版本更新后，回放仍只核对原操作；改变参数继续拒绝。
///
/// 数据构造：device=phone-a、media=rtc_A、function=call_stable；grant revision 1→2，binding generation 1→2。
/// 执行过程：首次派工落盘 → 更新同项目控制授权 → 新 sideband 回放同调用 → 再改任务文本。
/// 预期结果：两个连接得到相同操作/任务回执；改参拒绝；前台始终只执行一次。
#[test]
fn reconnect_with_new_grant_revision_recovers_the_original_receipt() {
    let fixture = Fixture::new(|| {}, accepted);
    let listener = fixture.upstream.try_clone().expect("上游句柄");
    let first = std::thread::spawn(move || {
        let mut socket = attach(&listener);
        turn(&mut socket, "item-stable", "请修复登录测试");
        call(&mut socket, "response-first", "call_stable", "修复登录测试");
        let receipt = output(&mut socket, "call_stable");
        continuation(&mut socket);
        receipt
    });
    let phone = fixture.bind(1);
    let original = first.join().expect("首个连接");
    let recovered_page = fixture.operations(&fixture.workspace);
    assert_eq!(
        recovered_page["result"]["operations"][0]["operation_id"], original["operation_id"],
        "手机必须从认证读取入口获取同一持久操作"
    );
    assert_eq!(
        recovered_page["result"]["operations"][0]["result"]["status"], "accepted",
        "读取任务接收回执不能变成执行完成"
    );
    assert_eq!(
        fixture.operations("another-project")["error"],
        "unauthorized",
        "越权项目不能读取操作记录"
    );
    assert_eq!(
        original["status"], "accepted",
        "第一次派工必须有真实接收回执"
    );
    drop(phone);
    fixture
        .server
        .replace_permissions(
            "phone-a",
            1,
            &[storage::MobileProjectAccess {
                workspace_id: fixture.workspace.clone(),
                access: storage::MobilePermission::Control,
            }],
        )
        .expect("更新授权版本");
    let listener = fixture.upstream.try_clone().expect("新连接上游");
    let second = std::thread::spawn(move || {
        let mut socket = attach(&listener);
        turn(&mut socket, "item-stable", "请修复登录测试");
        call(
            &mut socket,
            "response-recovered",
            "call_stable",
            "修复登录测试",
        );
        let receipt = output(&mut socket, "call_stable");
        continuation(&mut socket);
        call(
            &mut socket,
            "response-changed",
            "call_stable",
            "删除所有测试",
        );
        let conflict = output(&mut socket, "call_stable");
        assert_eq!(
            conflict["status"], "rejected",
            "授权更新不能允许同身份改变工具参数"
        );
        receipt
    });
    let _phone = fixture.bind(2);
    let recovered = second.join().expect("重连回读");
    assert_eq!(
        recovered, original,
        "权限版本不属于工具操作身份；当前授权通过后应恢复原回执"
    );
    assert_eq!(
        fixture.writes.load(Ordering::SeqCst),
        1,
        "重连、授权更新和改参均不得重新执行"
    );
}

/// 转写迟到和角色/任务混合调用不应丢失授权，也不能触发两次继续回复。
///
/// 数据构造：一个 response 含 set_behavior 与 create_agent；原始转写晚于 response.done。
/// 执行过程：函数请求 → Ping/Pong 确认尚未返回结果 → 转写到达 → Mac 回执 → 本地角色回执。
/// 预期结果：只按实际转写派工一次；Mac 不回答角色工具；本地结果到达前不续答，之后只续答一次。
#[test]
fn late_transcription_and_mixed_tools_share_one_continuation() {
    let fixture = Fixture::new(|| {}, accepted);
    let listener = fixture.upstream.try_clone().expect("上游句柄");
    let upstream = std::thread::spawn(move || {
        let mut socket = attach(&listener);
        send(
            &mut socket,
            json!({"type":"input_audio_buffer.speech_started","item_id":"item-late"}),
        );
        send(
            &mut socket,
            json!({"type":"input_audio_buffer.speech_stopped","item_id":"item-late"}),
        );
        send(
            &mut socket,
            json!({"type":"response.created","response":{"id":"response-mixed"}}),
        );
        send(
            &mut socket,
            json!({"type":"response.done","response":{"id":"response-mixed","status":"completed","output":[
                {"type":"function_call","call_id":"call_avatar","name":"set_behavior","arguments":"{}"},
                {"type":"function_call","call_id":"call_task","name":"create_agent","arguments":json!({"profile":"codex","task":"修复登录测试"}).to_string()}
            ]}}),
        );
        socket
            .send(tungstenite::Message::Ping(vec![1].into()))
            .expect("转写等待屏障");
        assert!(
            matches!(
                socket.read().expect("等待转写"),
                tungstenite::Message::Pong(_)
            ),
            "未收到转写前不能提前判定拒绝或执行"
        );
        send(
            &mut socket,
            json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"item-late","transcript":"请修复登录测试"}),
        );
        let result = output(&mut socket, "call_task");
        assert_eq!(result["status"], "accepted", "迟到的真实转写应支持明确派工");
        socket
            .send(tungstenite::Message::Ping(vec![2].into()))
            .expect("角色等待屏障");
        assert!(
            matches!(
                socket.read().expect("等待角色回执"),
                tungstenite::Message::Pong(_)
            ),
            "Mac 不得替角色输出或在角色完成前续答"
        );
        send(
            &mut socket,
            json!({"type":"conversation.item.added","item":{"type":"function_call_output","call_id":"call_avatar","output":"{\"applied\":true}"}}),
        );
        continuation(&mut socket);
        send(
            &mut socket,
            json!({"type":"conversation.item.added","item":{"type":"function_call_output","call_id":"call_avatar","output":"{\"applied\":true}"}}),
        );
        socket
            .send(tungstenite::Message::Ping(vec![3].into()))
            .expect("去重屏障");
        assert!(
            matches!(
                socket.read().expect("单次续答"),
                tungstenite::Message::Pong(_)
            ),
            "重复角色事件不得产生第二次 response.create"
        );
    });
    let _phone = fixture.bind(1);
    upstream.join().expect("迟到转写与混合工具");
    assert_eq!(
        fixture.writes.load(Ordering::SeqCst),
        1,
        "一组混合工具只创建一个真实任务"
    );
}

/// 插话发生在项目快照等待期间，旧转写对应的写工具不得越过提交边界。
///
/// 数据构造：快照回复受通道控制；第一回合请求创建任务，第二回合仅开始说话。
/// 执行过程：放行开聊概况 → 等待工具快照 → 上游发送新 speech_started → Ping/Pong 确认已处理 → 放行旧快照。
/// 预期结果：旧工具拒绝、前台投递数为零；新发言不触发取消已执行任务。
#[test]
fn interruption_before_snapshot_returns_prevents_submission() {
    let (waiting, waited) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let mut initial = true;
    let fixture = Fixture::new(
        move || {
            if initial {
                initial = false;
                return;
            }
            waiting.send(()).expect("快照已等待");
            // 5s bounds a failed test; successful release is driven by the protocol acknowledgement.
            released
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("放行快照");
        },
        accepted,
    );
    let listener = fixture.upstream.try_clone().expect("上游句柄");
    let upstream = std::thread::spawn(move || {
        let mut socket = attach(&listener);
        turn(&mut socket, "item-old", "请修复登录测试");
        call(&mut socket, "response-old", "call_old", "修复登录测试");
        waited
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("收到快照请求");
        send(
            &mut socket,
            json!({"type":"input_audio_buffer.speech_started","item_id":"item-new"}),
        );
        socket
            .send(tungstenite::Message::Ping(vec![1].into()))
            .expect("事件处理屏障");
        assert!(
            matches!(
                socket.read().expect("处理确认"),
                tungstenite::Message::Pong(_)
            ),
            "上游必须先处理插话再放行旧快照"
        );
        release.send(()).expect("恢复项目快照");
        output(&mut socket, "call_old")
    });
    let _phone = fixture.bind(1);
    let result = upstream.join().expect("插话场景");
    assert_eq!(
        result["status"], "rejected",
        "尚未提交的旧回合必须拒绝，不能返回接收或未知回执"
    );
    assert_eq!(
        fixture.writes.load(Ordering::SeqCst),
        0,
        "插话后的旧意图不得投递到前台"
    );
}

/// 执行开始后回执无法解析，应返回 unknown 并冻结本回合后续工具。
///
/// 数据构造：隔离 SQLite 的首个命令已占用执行租约；将该行 result_json 改成残缺 JSON。
/// 执行过程：前台开始 → 注入回执损坏 → 返还执行结果 → 同回合请求第二个工具。
/// 预期结果：首个和后续工具均 unknown；实际投递只有一次，原 reservation 仍存在。
#[test]
fn unreadable_receipt_after_dispatch_freezes_followup_tools() {
    let (started, observed) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let fixture = Fixture::new(
        || {},
        move |request| {
            started
                .send(request.source.operation_id.clone())
                .expect("执行已开始");
            // 5s is the same bounded protocol budget used by the upstream fixture.
            released
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("故障注入完成");
            accepted(request);
        },
    );
    let database = fixture.store.path().to_path_buf();
    let listener = fixture.upstream.try_clone().expect("上游句柄");
    let upstream = std::thread::spawn(move || {
        let mut socket = attach(&listener);
        turn(&mut socket, "item-write", "请修复登录测试");
        call(&mut socket, "response-write", "call_write", "修复登录测试");
        let operation = observed
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("真实前台已取得租约");
        let database = rusqlite::Connection::open(database).expect("隔离数据库");
        database
            .execute(
                "UPDATE mobile_commands SET result_json='{' WHERE operation_id=?1",
                [&operation],
            )
            .expect("仅损坏当前测试回执");
        release.send(()).expect("继续完成原请求");
        let first = output(&mut socket, "call_write");
        assert_eq!(
            first["status"], "unknown",
            "执行后无法核对结果不能返回可重试的 rejected"
        );
        continuation(&mut socket);
        call(
            &mut socket,
            "response-followup",
            "call_followup",
            "修复另一处测试",
        );
        let second = output(&mut socket, "call_followup");
        assert_eq!(
            second["status"], "unknown",
            "同回合出现 unknown 后必须阻断后续工具"
        );
        let state: String = database
            .query_row(
                "SELECT state FROM mobile_commands WHERE operation_id=?1",
                [&operation],
                |r| r.get(0),
            )
            .expect("原 reservation 保留");
        assert_eq!(state, "accepted", "读取损坏不应抹掉已执行请求的持久身份");
    });
    let _phone = fixture.bind(1);
    upstream.join().expect("未知回执场景");
    assert_eq!(
        fixture.writes.load(Ordering::SeqCst),
        1,
        "结果未知不能再投递任何后续写操作"
    );
}

/// OpenAI 实时函数调用经 Mac 唯一执行入口落盘；讨论不派工，同调用重放只查原回执。
///
/// 数据构造：真实本机 TLS/授权/命令数据库；回环上游发送转写与工具事件；
///   前台替身只接受 create_agent，返回 task_id=7 的接收回执，不模拟任务完成。
/// 执行过程：讨论中的建任务请求 → 明确修复 → 同 function call_id 重放 → 同 ID 改参。
/// 预期结果：讨论拒绝、明确请求只投递一次；accepted 不冒充完成；改参拒绝且无第二次副作用。
#[test]
fn realtime_tools_require_intent_and_recover_the_same_durable_operation() {
    let fixture = Fixture::new(|| {}, accepted);
    let observed_writes = Arc::clone(&fixture.writes);
    let listener = fixture.upstream.try_clone().expect("上游句柄");
    let upstream = std::thread::spawn(move || {
        let mut socket = attach(&listener);
        turn(&mut socket, "item-discussion", "这个思路不错");
        call(
            &mut socket,
            "response-discussion",
            "call_discussion",
            "修复登录测试",
        );
        let rejected = output(&mut socket, "call_discussion");
        assert_eq!(rejected["status"], "rejected", "讨论不能获得执行成功回执");
        assert_eq!(
            observed_writes.load(Ordering::SeqCst),
            0,
            "讨论阶段不能触达前台写操作"
        );
        continuation(&mut socket);
        turn(&mut socket, "item-execute", "请修复登录测试，保留鉴权逻辑");
        call(
            &mut socket,
            "response-execute",
            "call_execute",
            "修复登录测试，保留鉴权逻辑",
        );
        let accepted = output(&mut socket, "call_execute");
        assert_eq!(accepted["status"], "accepted", "正式回执只能说明任务已接收");
        assert_eq!(accepted["task_id"], 7, "任务身份必须来自执行层回执");
        assert_ne!(
            accepted["status"], "succeeded",
            "派工接收不能冒充任务执行完成"
        );
        assert_eq!(
            observed_writes.load(Ordering::SeqCst),
            1,
            "明确执行准确投递一次"
        );
        continuation(&mut socket);
        call(
            &mut socket,
            "response-replay",
            "call_execute",
            "修复登录测试，保留鉴权逻辑",
        );
        let replay = output(&mut socket, "call_execute");
        assert_eq!(
            replay, accepted,
            "相同函数调用必须恢复同一持久回执和操作身份"
        );
        assert_eq!(
            observed_writes.load(Ordering::SeqCst),
            1,
            "重复函数事件不能再派工"
        );
        continuation(&mut socket);
        call(
            &mut socket,
            "response-conflict",
            "call_execute",
            "删除所有测试",
        );
        let conflict = output(&mut socket, "call_execute");
        assert_eq!(conflict["status"], "rejected", "同调用身份改参数必须拒绝");
        assert_eq!(
            observed_writes.load(Ordering::SeqCst),
            1,
            "改参数冲突不得产生额外副作用"
        );
        accepted["operation_id"]
            .as_str()
            .expect("稳定操作身份")
            .to_owned()
    });
    let _phone = fixture.bind(1);
    let result = upstream.join();
    let operation = result.expect("实时工具闭环");
    let stored = storage::MobileCommandStore::new(fixture.store.path().to_path_buf())
        .get("phone-a", &operation, &fixture.workspace, "inspection")
        .expect("持久查询")
        .expect("实际命令记录");
    assert_eq!(stored.state, "succeeded", "命令已应用应有持久终态");
    assert_eq!(
        stored.result.expect("工具结果")["status"],
        "accepted",
        "持久命令终态与 Agent 任务完成必须分开"
    );
}

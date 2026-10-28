use super::*;
use serde_json::json;

/// 场景：长会话中的普通完成事件不对应任何原生问题，不能凭历史容量制造输入锁。
/// 构造：128条容量加1个普通turn，共129个结束事件，从未发送requestUserInput。
/// 执行：附着原线程，依次观察结束事件，再读取输入占有状态。
/// 预期：输入一直可用；历史容量不足不能伪造待答问题。
#[test]
fn ordinary_turn_history_never_creates_pending_input() {
    let mut input = NativeInputs::default();
    input.attach("thread");
    for i in 0..=CAPACITY {
        input.observe(&json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":format!("turn-{i}")}}}));
        assert!(
            !input.blocked.load(Ordering::Acquire),
            "第{}个普通turn不能制造问题",
            i + 1
        );
    }
}

/// 场景：长会话累计问题数超过有界历史窗口，已完成的问题不能永久占用未决容量。
/// 构造：同线程内连续129个不同RPC/turn/call；128是历史上限，额外1题跨过边界。
/// 执行：每题依次收到请求、精确工具输出、回合结束，再重放刚完成的请求。
/// 预期：每道当前题都阻塞普通输入；消费后解除，完成后的重放不能重新占用。
#[test]
fn completed_question_history_does_not_exhaust_pending_capacity() {
    let mut input = NativeInputs::default();
    input.attach("thread");
    for i in 0..=CAPACITY {
        let turn = format!("turn-{i}");
        let item = format!("call-{i}");
        let request = json!({"id":i,"method":"item/tool/requestUserInput","params":{
            "threadId":"thread","turnId":turn,"itemId":item
        }});
        input.observe(&request);
        assert!(
            input.blocked.load(Ordering::Acquire),
            "第{}个原问题必须占有输入",
            i + 1
        );
        input.observe(&json!({"method":"rawResponseItem/completed","params":{
            "threadId":"thread","turnId":turn,"item":{"type":"function_call_output","call_id":item}
        }}));
        input.observe(
            &json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":turn}}}),
        );
        input.observe(&request);
        assert!(
            !input.blocked.load(Ordering::Acquire),
            "第{}题消费后必须解除输入占有，重放不能重新阻塞",
            i + 1
        );
    }
}

/// 场景：历史窗口回收后重放远古请求，不应重新锁住当前原生输入。
/// 构造：129个真实start/completed回合，再发第0轮pending和第129轮的新题。
/// 执行：旧请求超过128条回放窗口仍必须被当前轮次身份拒绝，新题应正常锁输入。
/// 预期：旧题不阻塞，新题阻塞；新会话attach清空旧轮身份后可接受恢复的pending。
#[test]
fn old_turn_outside_history_window_cannot_hold_current_input() {
    let mut input = NativeInputs::default();
    input.attach("thread");
    for i in 0..=CAPACITY {
        for state in ["started", "completed"] {
            input.observe(&json!({"method":format!("turn/{state}"),"params":{"threadId":"thread","turn":{"id":format!("turn-{i}")}}}));
        }
    }
    let old = json!({"id":0,"method":"item/tool/requestUserInput","params":{"threadId":"thread","turnId":"turn-0","itemId":"call-0"}});
    input.observe(&old);
    assert!(
        !input.blocked.load(Ordering::Acquire),
        "窗口外旧题不能重新锁输入"
    );
    input.observe(
        &json!({"method":"turn/started","params":{"threadId":"thread","turn":{"id":"current"}}}),
    );
    let mut current = old.clone();
    current["params"]["turnId"] = "current".into();
    input.observe(&current);
    assert!(input.blocked.load(Ordering::Acquire), "当前题仍占有输入");
    input.attach("other");
    current["params"]["threadId"] = "other".into();
    input.observe(&current);
    assert!(
        input.blocked.load(Ordering::Acquire),
        "新附着可接受尚无start重放的pending"
    );
}

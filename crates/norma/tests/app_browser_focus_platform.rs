#[path = "../../app/src/app/browser/focus_policy.rs"]
mod focus_policy;
use focus_policy::{may_return_focus, FocusSnapshot};

/// 场景：延迟释放浏览器焦点，不能覆盖用户之后切窗或选择另一输入框的意图。
/// 数据构造：根窗A=10、根窗B=20；浏览器子焦点A=11、B=12；独立固定OS快照。
/// 执行过程：在相同排队意图下分别消费未变、切到B、A内改焦点及源不拥有焦点的命令。
/// 预期结果：仅A仍前台、焦点未变且确为该WebView所有时归还父窗；其余全部拒绝。
#[test]
fn native_focus_return_requires_current_user_intent_and_source_ownership() {
    let queued = FocusSnapshot {
        foreground: 10,
        focused: 11,
    };
    let cases = [
        (queued, queued, 10, true, true),
        (
            queued,
            FocusSnapshot {
                foreground: 20,
                focused: 11,
            },
            10,
            true,
            false,
        ),
        (
            queued,
            FocusSnapshot {
                foreground: 10,
                focused: 12,
            },
            10,
            false,
            false,
        ),
        (queued, queued, 10, false, false),
        (
            FocusSnapshot {
                foreground: 20,
                focused: 11,
            },
            FocusSnapshot {
                foreground: 20,
                focused: 11,
            },
            10,
            true,
            false,
        ),
    ];
    for (expected, current, parent, owned, allowed) in cases {
        assert_eq!(
            may_return_focus(expected, current, parent, owned), allowed,
            "迟到焦点决策必须尊重最新用户意图: expected={expected:?}, current={current:?}, parent={parent}, owned={owned}"
        );
    }
}

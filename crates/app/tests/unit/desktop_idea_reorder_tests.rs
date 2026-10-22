use super::*;
use storage::IdeaSource;

fn idea(id: &str, state: IdeaState) -> StoredIdea {
    StoredIdea {
        id: id.to_owned(),
        workspace_id: "w".to_owned(),
        body: id.to_owned(),
        source: IdeaSource::Manual,
        state,
        task_ref: None,
        assignee_area_id: None,
        assignee_node_id: None,
        position: 0,
        images: Vec::new(),
        created_at_unix: 0,
        updated_at_unix: 0,
    }
}

fn order(ideas: &[StoredIdea]) -> Vec<&str> {
    ideas.iter().map(|idea| idea.id.as_str()).collect()
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

/// 内存里的移动与库里 `move_before` 同语义，且不动已指派条目。
///
/// 数据构造：清单 [a(待办), x(已指派), b(待办), c(待办)]。
/// 执行过程：c 移到 a 之前；再把 a 移到末尾（所有待办之后）。
/// 预期结果：
///   - 正断言：第一次后 [c, a, x, b]；第二次后 [c, x, b, a]（a 排在最后一条待办之后）。
///   - 负断言：x 始终在清单里且不是待办；移到自身之前、移到非待办之前、已在原位均返回 false 且顺序不变。
#[test]
fn apply_move_matches_store_semantics() {
    let mut ideas = vec![
        idea("a", IdeaState::Todo),
        idea("x", IdeaState::Assigned),
        idea("b", IdeaState::Todo),
        idea("c", IdeaState::Todo),
    ];
    assert!(apply_move(&mut ideas, "c", Some("a")), "c 应移到 a 之前");
    assert_eq!(order(&ideas), ["c", "a", "x", "b"], "第一次移动后的顺序");
    assert!(apply_move(&mut ideas, "a", None), "a 应移到末尾");
    assert_eq!(order(&ideas), ["c", "x", "b", "a"], "第二次移动后的顺序");
    let before = order(&ideas).join(",");
    assert!(!apply_move(&mut ideas, "b", Some("b")), "不能移到自身之前");
    assert!(
        !apply_move(&mut ideas, "b", Some("x")),
        "不能移到已指派条目之前"
    );
    assert!(!apply_move(&mut ideas, "x", None), "已指派条目不可排序");
    assert!(!apply_move(&mut ideas, "a", None), "已在末尾不算变化");
    assert!(!apply_move(&mut ideas, "ghost", None), "不存在的条目");
    assert_eq!(order(&ideas).join(","), before, "失败的移动不应改变顺序");
}

/// `⌥↑/⌥↓`：上移放到上一条之前，下移越过下一条；到头不动。
///
/// 数据构造：待办 [a, b, c]。
/// 预期结果：
///   - b 上移 → 放到 a 之前，光标 0；b 下移 → 放到末尾（c 已是最后，下下条不存在），光标 2。
///   - a 下移 → 放到 c 之前（越过 b），光标 1。
///   - 负断言：a 上移、c 下移、越界下标都返回 None。
#[test]
fn keyboard_step_moves_one_slot_and_stops_at_edges() {
    let todo = ids(&["a", "b", "c"]);
    assert_eq!(
        keyboard_step(&todo, 1, Step::Up),
        Some((Some("a".to_owned()), 0)),
        "b 上移"
    );
    assert_eq!(
        keyboard_step(&todo, 1, Step::Down),
        Some((None, 2)),
        "b 下移到末尾"
    );
    assert_eq!(
        keyboard_step(&todo, 0, Step::Down),
        Some((Some("c".to_owned()), 1)),
        "a 下移越过 b"
    );
    assert_eq!(keyboard_step(&todo, 0, Step::Up), None, "队首不能再上移");
    assert_eq!(keyboard_step(&todo, 2, Step::Down), None, "队尾不能再下移");
    assert_eq!(keyboard_step(&todo, 9, Step::Up), None, "越界下标");
}

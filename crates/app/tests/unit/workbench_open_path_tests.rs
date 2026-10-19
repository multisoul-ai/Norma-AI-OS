use super::*;

fn pending() -> PendingOpen {
    PendingOpen {
        area_id: "area-a".into(),
        rel: "src/lib.rs".into(),
        line: Some(42),
        is_dir: false,
    }
}

/// 需要展开的目录链：文件只展开祖先；目录还要展开目录自身，使其子项可见。
///
/// 数据构造：
///   "a/b/c.rs" 是文件 → 祖先 a、a/b（2 层）
///   "a/b" 是目录     → a、a/b（2 层，含目录自身）
///   "top" 是根下目录  → 仅 top（1 层）
///
/// 预期结果：
///   - 正断言：文件链不含自身；目录链含自身。
///   - 负断言：文件 "a/b/c.rs" 的链不得含 "a/b/c.rs"。
#[test]
fn test_expand_chain_includes_directory_itself_but_not_file() {
    let file = expand_chain("a/b/c.rs", false);
    let dir = expand_chain("a/b", true);
    let top = expand_chain("top", true);

    assert_eq!(
        file,
        vec!["a".to_string(), "a/b".to_string()],
        "文件只展开祖先目录"
    );
    assert!(
        !file.contains(&"a/b/c.rs".to_string()),
        "文件自身不是可展开目录"
    );
    assert_eq!(
        dir,
        vec!["a".to_string(), "a/b".to_string()],
        "目录链应含目录自身"
    );
    assert_eq!(top, vec!["top".to_string()], "根下目录只需展开自身");
}

/// 待打开文件何时可以真正选中：目标项目已选中、其目录树已加载。
///
/// 数据构造：
///   pending.area_id = "area-a"
///   组合 saved_area / project_area / loaded 三个条件，每次只破坏其中一个。
///
/// 执行过程：对每个组合调用 `PendingOpen::ready`。
///
/// 预期结果：
///   - 三个条件全满足 → true
///   - 工作台仍停在别的项目（saved_area=area-b）→ false，否则会把文件选进错误的项目
///   - 目录树还是旧项目的（project_area=area-b，刚切换尚未重载）→ false
///   - 项目尚未加载（loaded=false）→ false，select 需要已加载的 Project
#[test]
fn test_pending_open_ready_requires_matching_loaded_project() {
    let p = pending();

    assert!(
        p.ready(Some("area-a"), Some("area-a"), true),
        "目标项目已选中且已加载时应就绪"
    );
    assert!(
        !p.ready(Some("area-b"), Some("area-a"), true),
        "工作台选中的是别的项目时不能就绪"
    );
    assert!(
        !p.ready(Some("area-a"), Some("area-b"), true),
        "目录树仍属于旧项目时不能就绪"
    );
    assert!(
        !p.ready(Some("area-a"), Some("area-a"), false),
        "项目尚未加载完成时不能就绪"
    );
    assert!(!p.ready(None, None, false), "全空状态不能就绪");
}

/// 待打开项的过期判断：用户在加载期间切走项目后，回来不得再跳到旧文件。
///
/// 数据构造：
///   pending.area_id = "area-a"（点击时 saved_area 被设为 area-a）
///
/// 执行过程：对不同的 saved_area 调用 `PendingOpen::is_stale`。
///
/// 预期结果：
///   - saved_area=area-a（仍停在目标项目）→ 未过期，继续等待加载。
///   - saved_area=area-b（用户已切到别的项目）→ 过期，应丢弃，避免切回 area-a 后意外跳转。
///   - saved_area=None（工作台已无项目）→ 过期。
#[test]
fn test_pending_open_is_stale_once_workbench_leaves_target_project() {
    let p = pending();

    assert!(!p.is_stale(Some("area-a")), "仍停在目标项目时不应过期");
    assert!(p.is_stale(Some("area-b")), "用户切到别的项目后应过期");
    assert!(p.is_stale(None), "工作台无项目时应过期");
}

/// 展开祖先目录：只返回目录，不含文件本身。
///
/// 数据构造：
///   "a/b/c.rs" → 目录链 a、a/b（2 层）
///   "top.rs"   → 无目录（0 层）
///   "a//b.rs"  → 连续分隔符产生的空段被忽略 → ["a"]
///
/// 预期结果：见断言。
#[test]
fn test_expand_ancestors_lists_only_parent_directories() {
    assert_eq!(
        expand_ancestors("a/b/c.rs"),
        vec!["a".to_string(), "a/b".to_string()],
        "应按由浅到深列出全部祖先目录，且不含文件本身"
    );
    assert!(
        expand_ancestors("top.rs").is_empty(),
        "根目录文件没有祖先目录"
    );
    assert_eq!(
        expand_ancestors("a//b.rs"),
        vec!["a".to_string()],
        "空路径段应被忽略"
    );
}

/// 行号换算：终端 1 起始 → 索引 0 起始，并夹在文本末行之内。
///
/// 数据构造：
///   text = "l1\nl2\nl3"，换行 2 个 → 3 行 → 最后索引 = 2
///   line=1 → 索引 0；line=3 → 索引 2；line=99 → 夹到 2；line=0（非法）→ 0
///
/// 预期结果：见断言。
#[test]
fn test_jump_index_converts_to_zero_based_and_clamps_to_last_line() {
    let text = "l1\nl2\nl3";

    assert_eq!(jump_index(1, text), 0, "第 1 行对应索引 0");
    assert_eq!(jump_index(3, text), 2, "第 3 行（末行）对应索引 2");
    assert_eq!(jump_index(99, text), 2, "超出行数时应夹到末行而不是越界");
    assert_eq!(jump_index(0, text), 0, "行号 0 视为第 1 行");
    assert_eq!(jump_index(5, ""), 0, "空文件只有索引 0");
}

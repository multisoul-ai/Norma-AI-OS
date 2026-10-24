use super::ui::avatar_initial;

/// 头像首字母：英文转大写、中文取首字、空名回退问号。
///
/// 数据构造：「zhou kai」「周凯」「  ada」（含前导空白）与空串、纯空白。
/// 预期结果：正断言：Z、周、A；负断言：空串与纯空白得到 "?"，不 panic。
#[test]
fn test_avatar_initial() {
    assert_eq!(avatar_initial("zhou kai"), "Z", "英文名取首字母并大写");
    assert_eq!(avatar_initial("周凯"), "周", "中文名取第一个字");
    assert_eq!(avatar_initial("  ada"), "A", "应忽略前导空白");
    assert_eq!(avatar_initial(""), "?", "空名回退为问号");
    assert_eq!(avatar_initial("   "), "?", "纯空白回退为问号");
}

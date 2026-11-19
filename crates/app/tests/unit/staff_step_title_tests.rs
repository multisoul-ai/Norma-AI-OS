use super::*;

fn norma(words: &[&str]) -> StepTitle {
    StepTitle::Norma(words.iter().map(|word| (*word).to_owned()).collect())
}

/// 场景：已知 norma 子命令显示成人话，未知的只显示命令词，工具名原样；任何情况下都没有路径。
/// 数据构造：brain agents、brain task propose、一个未知子命令、Bash 工具。
/// 执行过程：逐个调用 label 取展示文案。
/// 预期结果：前两个走 i18n 表（不等于原始命令词）；未知的是「norma brain frobnicate」；工具名原样；结果里没有「/」。
#[test]
fn known_commands_are_translated_and_unknown_ones_stay_command_words() {
    let agents = label(&norma(&["brain", "agents"]));
    assert_ne!(agents, "norma brain agents", "已知命令应翻译成人话");
    assert!(
        !agents.contains("staff.step"),
        "缺少翻译键会把键名漏到界面上：{agents}"
    );
    let propose = label(&norma(&["brain", "task", "propose"]));
    assert_ne!(propose, agents, "不同命令有不同标题");
    assert_eq!(
        label(&norma(&["brain", "frobnicate"])),
        "norma brain frobnicate",
        "未知子命令只显示命令词"
    );
    assert_eq!(
        label(&StepTitle::Tool("skill".into())),
        "skill",
        "工具名原样显示"
    );
    for title in [agents, propose] {
        assert!(!title.contains('/'), "标题不能含路径：{title}");
    }
}

/// 场景：带路径的工具步骤，名称与详情分开展示，详情不污染名称。
/// 数据构造：ToolAt("Read", "…/app/src/lib.rs:42") 与纯 Tool("Read")。
/// 预期结果：两者的名称都是「Read」；前者 detail 为路径，后者为 None（不显示空占位）。
#[test]
fn tool_steps_show_detail_separately_from_the_name() {
    let with_path = StepTitle::ToolAt("Read".into(), "…/app/src/lib.rs:42".into());
    assert_eq!(label(&with_path), "Read", "名称只含工具名");
    assert_eq!(
        detail(&with_path),
        Some("…/app/src/lib.rs:42"),
        "详情是路径"
    );
    let bare = StepTitle::Tool("Read".into());
    assert_eq!(label(&bare), "Read", "纯工具名不变");
    assert_eq!(detail(&bare), None, "没有路径时不显示详情");
}

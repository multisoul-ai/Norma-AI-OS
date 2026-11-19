use norma::protocol::{ReplyTrigger, StaffPart, StepStatus, StepTitle, Task};

fn step(reply: &norma::protocol::StaffReply, id: &str) -> (StepTitle, StepStatus) {
    reply
        .parts
        .iter()
        .find_map(|part| match part {
            StaffPart::Step {
                id: found,
                title,
                status,
                ..
            } if found == id => Some((title.clone(), *status)),
            _ => None,
        })
        .expect("step exists")
}

/// 场景：工具步骤标题要带上读了哪个文件、进了哪个目录、查了哪个路径，而不是只剩工具名。
/// 数据构造（取自实际截图里的 rg、cd、Read 三类步骤）：
///   Read：ACP locations 给出绝对路径 + 行号，rawInput 另有 file_path；
///   Read（无 locations）：只有 rawInput.file_path；
///   cd：命令 `cd /a/b/c/d/e && ls`，没有 locations；
///   rg：命令 `rg -n "pattern" docs/plans`，最后一个非参数词是查询路径；
///   rg 只有模式：没有路径时回落到模式；
///   zsh 包装：`/bin/zsh -lc 'cd /x/y && rg foo'` 要穿透外壳取目录；
///   什么线索都没有：Bash + `{}`。
/// 执行过程：逐个调用 step_title_at(name, input, locations 路径)。
/// 预期结果：
///   - 有 locations 时优先用它，并保留 :行号
///   - 没有 locations 时取 rawInput 的 file_path
///   - cd 显示目录；超过三段的路径只保留末三段并以「…/」开头，绝对前缀不进快照
///   - rg 显示最后一个路径参数；只有模式时不显示；git -C 取目录而不是子命令
///   - 线索全无时退回只有工具名，不编造路径
#[test]
fn step_titles_carry_the_file_or_directory_they_touch() {
    use norma::protocol::step_title_at;
    let at = |name: &str, detail: &str| StepTitle::ToolAt(name.into(), detail.into());
    assert_eq!(
        step_title_at(
            Some("Read"),
            Some(r#"{"file_path":"/Users/me/p/crates/app/src/main.rs"}"#),
            Some("/Users/me/p/crates/app/src/lib.rs:42"),
        ),
        at("Read", "…/app/src/lib.rs:42"),
        "locations 优先并保留行号，且只留末三段"
    );
    assert_eq!(
        step_title_at(Some("Read"), Some(r#"{"file_path":"/p/a.md"}"#), None),
        at("Read", "/p/a.md"),
        "没有 locations 时取 rawInput.file_path；不足三段原样保留"
    );
    assert_eq!(
        step_title_at(
            Some("cd /a/b/c/d/e && ls"),
            Some(r#"{"command":"cd /a/b/c/d/e && ls"}"#),
            None
        ),
        at("cd", "…/c/d/e"),
        "cd 显示目录"
    );
    assert_eq!(
        step_title_at(
            Some("rg"),
            Some(r#"{"command":"rg -n \"pattern\" docs/plans"}"#),
            None
        ),
        at("rg", "docs/plans"),
        "rg 显示查询路径而不是模式"
    );
    assert_eq!(
        step_title_at(Some("rg"), Some(r#"{"command":"rg -n needle"}"#), None),
        StepTitle::Tool("rg".into()),
        "只有查询模式、没有路径时不显示：模式不是文件或目录"
    );
    assert_eq!(
        step_title_at(
            Some("git"),
            Some(r#"{"command":"git -C /a/b/c status"}"#),
            None
        ),
        at("git", "/a/b/c"),
        "取最后一个像路径的词，而不是最后一个参数 status"
    );
    assert_eq!(
        step_title_at(
            Some("zsh"),
            Some(r#"{"command":["/bin/zsh","-lc","cd /x/y && rg foo"]}"#),
            None
        ),
        at("zsh", "/x/y"),
        "穿透 zsh -lc 外壳取 cd 的目录"
    );
    assert_eq!(
        step_title_at(Some("Bash"), Some("{}"), None),
        StepTitle::Tool("Bash".into()),
        "没有任何线索时只留工具名，不编造路径"
    );
}

/// 场景：参数、凭据、URL、环境变量赋值、重定向目标、搜索模式都不是文件或目录，不得进入快照。
/// 数据构造：
///   python3 脚本带 `--token abc`；curl 带 URL 与 query；Grep 只有 pattern 字段；
///   `export API_KEY=v && ls`；`echo hi > /etc/out.txt`；`cd -`；Read 的 file_path 是空串。
/// 执行过程：逐个调用 step_title_at。
/// 预期结果：全部只留工具名（Tool），不产生 ToolAt；快照里找不到 abc、token、API_KEY、hunter2、/etc。
#[test]
fn step_titles_never_carry_arguments_or_secrets() {
    use norma::protocol::step_title_at;
    let cases: [(&str, &str); 7] = [
        ("python3", r#"{"command":"python3 script.py --token abc"}"#),
        (
            "curl",
            r#"{"command":"curl -H x https://x.io/api?token=T"}"#,
        ),
        ("Grep", r#"{"pattern":"password=hunter2"}"#),
        ("bash", r#"{"command":"export API_KEY=v && ls"}"#),
        ("echo", r#"{"command":"echo hi > /etc/out.txt"}"#),
        ("cd", r#"{"command":"cd -"}"#),
        ("Read", r#"{"file_path":""}"#),
    ];
    for (name, input) in cases {
        let title = step_title_at(Some(name), Some(input), None);
        assert_eq!(
            title,
            StepTitle::Tool(name.into()),
            "{name} 的入参不是文件或目录，只能留工具名：{input}"
        );
    }
}

/// 场景：同一步骤的后续事件没有路径时，不能把已知的文件或目录与工具名覆盖掉。
/// 数据构造：步骤 a 先收到 ToolAt("Read","…/a.rs:3")，再收到只有 title 的完成事件 Tool("Read")，
///   再收到名称是占位值的 Tool("tool")；步骤 b 先 Tool("Read")，后来才收到只带路径的 ToolAt("tool", "…/b.rs")；
///   步骤 c 带路径后失败。
/// 预期结果：
///   - a 的标题始终是 ToolAt("Read","…/a.rs:3")
///   - b 的标题合并为 ToolAt("Read","…/b.rs")，名称取旧值，路径取新值
///   - c 失败后标题仍带路径，状态为 Failed
#[test]
fn later_events_without_a_path_keep_the_known_detail() {
    let mut task = Task::new("t", "w", "p", "x");
    let reply = task.begin_reply(ReplyTrigger::User);
    let at = |name: &str, detail: &str| StepTitle::ToolAt(name.into(), detail.into());
    reply.upsert_step(
        "a",
        Some(at("Read", "…/a.rs:3")),
        Some(StepStatus::Running),
        None,
    );
    reply.upsert_step(
        "a",
        Some(StepTitle::Tool("Read".into())),
        Some(StepStatus::Completed),
        Some(3),
    );
    reply.upsert_step("a", Some(StepTitle::Tool("tool".into())), None, None);
    assert_eq!(
        step(reply, "a"),
        (at("Read", "…/a.rs:3"), StepStatus::Completed),
        "同名或占位名的无路径事件不得覆盖已知详情"
    );
    reply.upsert_step(
        "b",
        Some(StepTitle::Tool("Read".into())),
        Some(StepStatus::Running),
        None,
    );
    reply.upsert_step("b", Some(at("tool", "…/b.rs")), None, None);
    assert_eq!(
        step(reply, "b").0,
        at("Read", "…/b.rs"),
        "晚到的路径补上，占位名不覆盖已知工具名"
    );
    reply.upsert_step(
        "c",
        Some(at("Read", "…/c.rs")),
        Some(StepStatus::Running),
        None,
    );
    reply.upsert_step("c", None, Some(StepStatus::Failed), Some(9));
    assert_eq!(
        step(reply, "c"),
        (at("Read", "…/c.rs"), StepStatus::Failed),
        "失败态保留路径"
    );
}

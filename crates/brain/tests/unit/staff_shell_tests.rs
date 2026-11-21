use super::{command_allowed, read_inside_project, staff_command_allowed};
use serde_json::json;
use std::path::Path;

fn bin() -> &'static Path {
    Path::new("/bin/norma")
}

/// 场景：幕僚只能执行自己的 `norma brain` 命令，复合 shell 和其他程序都拒绝。
///
/// 数据构造：norma 的绝对路径是 `/bin/norma`。合法命令是这个路径加 `brain`。
/// 执行过程：分别判定合法命令、分号、命令替换、管道、Codex 对象、zsh 外壳和其他绝对路径。
/// 预期结果：
///   - 正断言：直接 argv、引号包住的路径、`zsh -lc` 外壳、Codex 的 `normaShellCommand` 都允许
///   - 负断言：`; rg ~`、`$(...)`、`| sh`、`/bin/bash`、相对路径 `norma` 都拒绝
#[test]
fn staff_shell_accepts_only_absolute_norma_brain() {
    assert!(
        command_allowed("'/bin/norma' brain agents --workspace w", bin()),
        "引号里的绝对路径加 brain 是合法命令"
    );
    assert!(
        command_allowed("zsh -lc \"'/bin/norma' brain agents\"", bin()),
        "zsh -lc 外壳剥掉后仍是 norma brain"
    );
    let codex = json!({"command": {"normaShellCommand": "zsh -lc \"/bin/norma brain agents\""}});
    assert!(
        staff_command_allowed(&codex, bin()),
        "Codex 把命令放在 normaShellCommand 里，解包后同样允许"
    );
    assert!(
        !command_allowed("'/bin/norma' brain agents; rg ~", bin()),
        "分号后面的 rg 必须整条拒绝"
    );
    assert!(
        !command_allowed("'/bin/norma' brain $(echo agents)", bin()),
        "命令替换必须拒绝"
    );
    assert!(
        !command_allowed("/bin/norma brain agents\ntrue", bin()),
        "未引用的换行会开始第二条命令"
    );
    assert!(
        command_allowed(
            "/bin/norma brain task assign --prompt 'first\nsecond'",
            bin()
        ),
        "引用内换行仍属于同一条派发指令"
    );
    assert!(
        !staff_command_allowed(
            &json!({"command": {"normaShellCommand": "/bin/norma brain agents"},
                    "additionalPermissions": {"network": {"enabled": true}}}),
            bin()
        ),
        "放行单条本地命令不等于授予附带权限"
    );
    assert!(
        !command_allowed("'/bin/norma' brain agents | sh", bin()),
        "管道必须拒绝"
    );
    assert!(
        !command_allowed("'/bin/bash' brain agents", bin()),
        "不是 norma 的绝对路径必须拒绝"
    );
    assert!(
        !command_allowed("norma brain agents", bin()),
        "相对路径不能当成已核对的 norma"
    );
    assert!(
        !read_inside_project("/etc/hosts", Path::new("/work/project")),
        "项目外的绝对路径不能读"
    );
    assert!(
        read_inside_project("src/main.rs", Path::new("/work/project")),
        "项目内的相对路径可以读"
    );
    assert!(
        !read_inside_project("../secrets", Path::new("/work/project")),
        "用 .. 跳出项目必须拒绝"
    );
}

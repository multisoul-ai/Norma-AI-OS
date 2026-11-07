use brain::acp::gate::{Gate, PermissionRequest, ReadRoots, Verdict};
use std::path::Path;

fn read(path: &Path) -> PermissionRequest {
    PermissionRequest {
        tool: "Read".into(),
        command: None,
        path: Some(path.to_string_lossy().into_owned()),
    }
}

/// 场景：全局幕僚已在运行，用户打开第二项目后关闭第一项目。
/// 构造：真实甲乙丙目录各含普通文件，乙另含凭据；两个Gate克隆模拟主/审批会话。
/// 执行：初始只授权甲，热更新为甲乙，再更新为仅乙；在各阶段调用生产judge。
/// 预期：新增项目立即可读，旧cwd被撤销，未打开目录/凭据/目录读取始终被拒绝。
#[test]
fn running_sessions_follow_open_project_roots_without_restarting() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let alpha = temp.path().join("alpha");
    let beta = temp.path().join("beta");
    let other = temp.path().join("other");
    for root in [&alpha, &beta, &other] {
        std::fs::create_dir(root).expect("应创建项目目录");
        std::fs::write(root.join("notes.txt"), "public notes").expect("应创建普通文件");
    }
    std::fs::write(beta.join(".env"), "test fixture").expect("应创建凭据路径夹具");
    let roots = ReadRoots::new(vec![alpha.clone()]);
    let main = Gate::new("/bin/norma".into(), alpha.clone(), roots.clone());
    let approval = main.clone();
    assert_eq!(
        main.judge(&read(&alpha.join("notes.txt"))),
        Verdict::Allow,
        "已打开甲可读"
    );
    assert!(
        matches!(
            main.judge(&read(&beta.join("notes.txt"))),
            Verdict::Reject(_)
        ),
        "尚未打开乙不可读"
    );
    roots.replace(vec![alpha.clone(), beta.clone()]);
    assert_eq!(
        approval.judge(&read(&beta.join("notes.txt"))),
        Verdict::Allow,
        "现有审批会话立即获得乙根目录"
    );
    assert!(
        matches!(
            approval.judge(&read(&other.join("notes.txt"))),
            Verdict::Reject(_)
        ),
        "未打开丙仍不可读"
    );
    assert!(
        matches!(
            approval.judge(&read(&beta.join(".env"))),
            Verdict::Reject(_)
        ),
        "新根中的凭据仍被拒绝"
    );
    assert!(
        matches!(approval.judge(&read(&beta)), Verdict::Reject(_)),
        "仍只允许逐文件读取"
    );
    roots.replace(vec![beta.clone()]);
    assert!(
        matches!(
            main.judge(&read(&alpha.join("notes.txt"))),
            Verdict::Reject(_)
        ),
        "关闭甲后不能因旧cwd继续授权"
    );
    assert!(
        matches!(
            main.judge(&read(Path::new("notes.txt"))),
            Verdict::Reject(_)
        ),
        "相对路径按原cwd解析，不能回退到乙"
    );
    assert_eq!(
        main.judge(&read(&beta.join("notes.txt"))),
        Verdict::Allow,
        "乙仍可绝对路径读取"
    );
    roots.replace(Vec::new());
    assert!(
        matches!(
            approval.judge(&read(&beta.join("notes.txt"))),
            Verdict::Reject(_)
        ),
        "清空根集合后全部撤销"
    );
}

/// 场景：后端可能展开用户目录/环境变量，闸门不能把这些路径当普通相对路径授权。
/// 构造：项目内真的存在名为 ~ 和 $HOME 的目录，两者各有一个普通文件。
/// 执行：使用原始前缀路径申请读取，同时验证普通相对文件仍可读。
/// 预期：即使字面文件存在，歧义前缀仍被拒绝，保留原单项目闸门边界。
#[test]
fn ambiguous_shell_prefixes_remain_rejected_with_real_literal_files() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    std::fs::write(temp.path().join("notes.txt"), "normal").expect("应创建普通文件");
    let gate = Gate::for_project("/bin/norma".into(), temp.path().into());
    assert_eq!(
        gate.judge(&read(Path::new("notes.txt"))),
        Verdict::Allow,
        "普通cwd相对文件仍可读"
    );
    for prefix in ["~", "$HOME"] {
        std::fs::create_dir(temp.path().join(prefix)).expect("应创建字面前缀目录");
        let relative = Path::new(prefix).join("notes.txt");
        std::fs::write(temp.path().join(&relative), "literal").expect("字面路径应真实存在");
        assert!(
            matches!(gate.judge(&read(&relative)), Verdict::Reject(_)),
            "原始前缀{prefix}有扩展歧义，即使字面文件存在也应拒绝"
        );
    }
}

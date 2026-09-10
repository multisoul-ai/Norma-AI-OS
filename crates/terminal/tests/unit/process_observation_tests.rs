use super::*;
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug)]
struct NoFallback;
impl ChildKiller for NoFallback {
    fn kill(&mut self) -> std::io::Result<()> {
        Ok(())
    }
    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self)
    }
}

/// Darwin参数读取只解析argc指定的argv，不返回环境变量或截断数据。
///
/// 数据构造：argc=3，argv为shell/-c/完整命令；末尾另有两个环境字段。
/// 执行过程：解析真实布局fixture → 缺NUL、负argc、过量argc、缺可执行路径分别重试。
/// 预期结果：三个原始参数保留空格；环境和所有不完整/越界输入不被接受。
#[test]
fn procargs_parser_excludes_environment_and_rejects_truncation() {
    let mut bytes = 3_i32.to_ne_bytes().to_vec();
    bytes.extend_from_slice(b"/bin/zsh\0\0/bin/zsh\0-c\0printf 'a  b'\0TOKEN=private\0PWD=/tmp\0");
    let parsed = observation::parse_procargs(&bytes).expect("完整argv");
    assert_eq!(
        parsed,
        ["/bin/zsh", "-c", "printf 'a  b'"],
        "完整命令不能压缩内部空格"
    );
    assert!(
        !parsed.iter().any(|s| s.contains("private")),
        "环境不得从读取接口泄漏"
    );
    let mut truncated = 3_i32.to_ne_bytes().to_vec();
    truncated.extend_from_slice(b"/bin/zsh\0\0/bin/zsh\0-c\0printf");
    assert!(
        observation::parse_procargs(&truncated).is_none(),
        "缺最后NUL不能截断后匹配"
    );
    bytes[..4].copy_from_slice(&(-1_i32).to_ne_bytes());
    assert!(
        observation::parse_procargs(&bytes).is_none(),
        "负argc不能分配内存"
    );
    // Maximum accepted count is 4096; one more must fail before allocating argv.
    bytes[..4].copy_from_slice(&4097_i32.to_ne_bytes());
    assert!(
        observation::parse_procargs(&bytes).is_none(),
        "超限argc不能读取环境补齐"
    );
    assert!(
        observation::parse_procargs(&3_i32.to_ne_bytes()).is_none(),
        "缺可执行路径必须失败"
    );
}

/// 仅原PTY审批前不存在的新直属子进程可作执行证据，既有进程与树外同命令均无效。
///
/// 数据构造：父shell先启动sleep30；树外shell和稍后树内shell具有完全相同argv。
///   30s保持进程存活，观测预算2s=20次×100ms，不是生产等待时长。
/// 执行过程：捕获PTY出生身份 → 等既有子进程 → baseline → 树外匹配失败 → 启动树内 → 匹配 → 模拟父PID复用。
/// 预期结果：准备前/既有/树外/复用身份不匹配；准确新子进程匹配且不能接受命令前缀。
#[test]
fn child_observation_is_bound_to_birth_parent_and_new_process() {
    let mut parent = Command::new("/bin/sh")
        .args([
            "-c",
            "sleep 30 & read command; /bin/sh -c \"$command\" & wait",
        ])
        .stdin(Stdio::piped())
        .spawn()
        .expect("隔离父进程");
    let managed = capture(Some(parent.id()));
    let deadline = Instant::now() + Duration::from_secs(2);
    while macos::child_pids(parent.id() as i32).is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut observer = ChildProcessObserver::from_managed(managed).expect("真实PTY身份");
    let command = "sleep 30; : norma-observation";
    let mut outsider = Command::new("/bin/sh")
        .args(["-c", command])
        .spawn()
        .expect("树外同命令");
    let matches = |args: &[String]| args == ["/bin/sh", "-c", command];
    let unprepared = observer.matches_new_child(matches);
    let prepared = observer.prepare();
    let existing = observer.matches_new_child(|_| true);
    let unrelated = observer.matches_new_child(matches);
    writeln!(parent.stdin.as_mut().expect("父stdin"), "{command}").expect("启动唯一新子进程");
    let deadline = Instant::now() + Duration::from_secs(2);
    let found = loop {
        if observer.matches_new_child(matches) {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let prefix = observer.matches_new_child(|args| args == ["/bin/sh", "-c", "sleep 30"]);
    // A reused PID differs in birth time; preserve PID and perturb only its recorded microsecond.
    let stale = ManagedProcess {
        identity: managed
            .identity
            .map(|i| i.with_different_start_time_for_test()),
        ..managed
    };
    let reused = ChildProcessObserver::from_managed(stale).is_some_and(|mut p| p.prepare());
    // Clean both test-owned trees even when evidence assertions fail.
    terminate_or_signal(managed, &mut NoFallback);
    terminate_or_signal(capture(Some(outsider.id())), &mut NoFallback);
    let _ = parent.wait();
    let _ = outsider.wait();
    assert!(prepared, "真实PTY必须能建立完整baseline");
    assert!(!unprepared, "未准备不得生成证据");
    assert!(!existing, "审批前已存在的sleep不能匹配");
    assert!(!unrelated, "树外相同argv不能匹配");
    assert!(found, "原PTY新增直属shell应准确匹配");
    assert!(!prefix, "命令前缀不能替代完整argv");
    assert!(!reused, "父PID出生身份不同不能继续观察");
}

/// CLI经启动器运行时，观察范围只下移到唯一匹配的直属进程，并持续保留启动器出生身份约束。
///
/// 数据构造：launcher含sleep和worker两个孩子；worker先有sleep，再按输入启动准确shell。
///   sleep30用于保持进程，2s=20次×100ms观测预算；所有进程均为本测试创建。
/// 执行过程：等待两层树→歧义选择拒绝→选择worker及baseline→启动准确子进程→父进程退出。
/// 预期结果：既有孩子不是执行证据；新命令可匹配，含糊选择/启动器退出后均不可匹配。
#[test]
fn child_observation_follows_only_one_verified_launcher_child() {
    let worker = "sleep 30 & read command; /bin/sh -c \"$command\" & wait";
    let mut parent = Command::new("/bin/sh")
        .args([
            "-c",
            "sleep 30 & /bin/sh -c \"$NORMA_OBSERVER_WORKER\" <&0 & wait",
        ])
        .env("NORMA_OBSERVER_WORKER", worker)
        .stdin(Stdio::piped())
        .spawn()
        .expect("隔离启动器");
    let managed = capture(Some(parent.id()));
    let deadline = Instant::now() + Duration::from_secs(2);
    while macos::child_pids(parent.id() as i32).len() != 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let children: Vec<_> = macos::child_pids(parent.id() as i32)
        .into_iter()
        .map(|pid| capture(Some(pid as u32)))
        .collect();
    let mut ambiguous = ChildProcessObserver::from_managed(managed).expect("原身份");
    let rejected_ambiguous = !ambiguous.prepare_with_child(|_| true);
    let mut observer = ChildProcessObserver::from_managed(managed).expect("原身份");
    let prepared = observer.prepare_with_child(|args| args == ["/bin/sh", "-c", worker]);
    let repeated = observer.prepare_with_child(|_| true);
    let command = "sleep 30; : norma-launcher-observation";
    let matches = |args: &[String]| args == ["/bin/sh", "-c", command];
    let before = observer.matches_new_child(matches);
    writeln!(parent.stdin.as_mut().expect("启动器输入"), "{command}").expect("新命令");
    let deadline = Instant::now() + Duration::from_secs(2);
    let found = loop {
        if observer.matches_new_child(matches) {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let _ = parent.kill();
    let _ = parent.wait();
    let orphan = observer.matches_new_child(matches);
    // Parent has exited; reclaim the exact captured test children rather than following reparented PIDs.
    for child in children {
        terminate_or_signal(child, &mut NoFallback);
    }
    assert!(rejected_ambiguous, "两个匹配孩子不能任选其一");
    assert!(prepared, "唯一worker应被准确绑定");
    assert!(!repeated, "不能继续下移观察根或重置已捕获的baseline");
    assert!(!before, "启动器已有进程不能充当新命令");
    assert!(found, "唯一worker的新直属shell必须可观察");
    assert!(!orphan, "启动器已退出后其孩子不能继续确认旧请求");
}

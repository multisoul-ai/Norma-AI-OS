use std::{path::PathBuf, process::Command};
pub fn executable() -> PathBuf {
    let program = if cfg!(windows) {
        "python"
    } else {
        "/usr/bin/python3"
    };
    let result = Command::new(program)
        .args(["-c", "import sys; print(sys.executable)"])
        .output()
        .expect("协议夹具需要 runner 的真实 Python");
    assert!(result.status.success(), "Python 路径查询必须成功");
    let path = PathBuf::from(
        String::from_utf8(result.stdout)
            .expect("Python 路径为 UTF-8")
            .trim(),
    );
    assert!(path.is_absolute(), "模型配置夹具使用实际绝对 Python 路径");
    path
}

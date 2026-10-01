use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use storage::{AppStore, DatabasePaths, MobilePermission, MobileProjectAccess, ProjectStore};

struct Gateway(Child);
impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// 手机正式 Swift Store 经本机中继与真实 Mac .app/已登录 Codex 完成任务和冻结成果回传。
/// 数据构造：隔离App数据库、项目、Codex配置/会话，1个Alice定义、1个控制授权；只链接已有登录。
/// 执行过程：真实安装包启动→Computer Use打开指定项目/手机服务→正式网关邀请→Swift派工/恢复/成果断言。
/// 预期结果：真实文件与回复通过Swift公共接口到原线程；不伪造Hook/执行结果，不触碰用户项目/个人会话。
/// 预算：界面准备最多600秒；Swift内部真实任务180秒。运行仅显式启用，常规测试不调用模型。
#[test]
#[ignore = "requires explicit real Codex login, isolated Mac bundle and Computer Use"]
fn real_codex_task_returns_through_thread_relay() {
    assert_eq!(
        std::env::var("NORMA_THREAD_CODEX_REAL_L3").as_deref(),
        Ok("1"),
        "需明确授权本地真实Codex验收"
    );
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let directory = tempfile::Builder::new()
        .prefix("norma-thread-real-")
        .tempdir()
        .expect("独立验收目录")
        .keep();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .expect("只允许本用户读取临时凭据");
    let project = directory.join("Codex Task");
    let profile = directory.join("codex-home");
    fs::create_dir(&project).expect("隔离项目");
    // macOS /var aliases /private/var; Codex checks the physical PTY working directory.
    let project = project
        .canonicalize()
        .expect("信任项必须与真实工作目录一致");
    fs::create_dir(&profile).expect("隔离Codex配置");
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&project)
            .status()
            .unwrap()
            .success(),
        "真实冻结成果需要Git项目"
    );
    let original = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("用户目录")).join(".codex")
        });
    assert!(
        original.join("auth.json").is_file(),
        "须有已登录Codex凭据；不得读取或输出密钥"
    );
    std::os::unix::fs::symlink(original.join("auth.json"), profile.join("auth.json"))
        .expect("私有符号链接复用登录");
    fs::write(profile.join("config.toml"), format!(
        "check_for_update_on_startup = false\nsandbox_mode = \"workspace-write\"\napproval_policy = \"never\"\n[features]\napps = false\n[projects.{}]\ntrust_level = \"trusted\"\n",
        serde_json::to_string(&project).unwrap())).expect("仅信任本轮隔离项目，不加载个人MCP/Hook");
    fs::write(profile.join(".zshrc"), "").expect("隔离shell启动配置");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("选择本地端口");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let launched = Command::new("bash")
        .arg(repository.join("scripts/run-norma-mobile-desktop-l3.sh"))
        .env(
            "NORMA_MOBILE_L3_BUNDLE_ID",
            "com.norma.thread-codex-validation",
        )
        .env("NORMA_THREAD_REAL_CODEX_HOME", &profile)
        .env("NORMA_CLOUD_LOCAL_PORT", port.to_string())
        .output()
        .expect("构建并启动隔离App");
    assert!(
        launched.status.success(),
        "真实App启动失败：{}",
        String::from_utf8_lossy(&launched.stderr)
    );
    let output = String::from_utf8_lossy(&launched.stdout);
    let app_root = PathBuf::from(output.lines().last().expect("隔离App路径"));
    assert!(
        app_root.starts_with("/tmp")
            && app_root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("norma-mobile-desktop-l3."),
        "仅允许本脚本的隔离App"
    );
    fs::write(
        directory.join("app-root"),
        app_root.as_os_str().as_encoded_bytes(),
    )
    .expect("保存公开App位置");
    let bundle = app_root.join("Norma Mobile Validation.app");
    let mut hooks = norma::agent_hook::default_install_paths();
    hooks.codex_hooks = profile.join("hooks.json");
    norma::agent_hook::install_hook_config(
        norma::agent_hook::HookTarget::Codex,
        &bundle.join("Contents/MacOS/multisoul-agent-hook"),
        &hooks,
    )
    .expect("只安装隔离Codex观察Hook");
    let paths = DatabasePaths::isolated_test(&app_root.join("data"));
    let definitions = ProjectStore::initialize_for_database_paths(&paths, &project)
        .expect("隔离项目库")
        .agent_definition_store();
    let definition = definitions
        .create(&storage::NewAgentDefinition {
            name: "Alice".into(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: String::new(),
        })
        .expect("具名Codex");
    eprintln!("Real thread evidence: {}", directory.display());
    eprintln!("Real App: {}", bundle.display());
    eprintln!(
        "Computer Use: open project {}, enable Mobile Connection",
        project.display()
    );
    let identity_path = app_root.join("data/thread-real-public-identity.json");
    // 600s is UI setup allowance, not an execution retry or network timeout.
    wait(Duration::from_secs(600), || identity_path.is_file());
    let identity: Value =
        serde_json::from_slice(&fs::read(identity_path).unwrap()).expect("只读取App公开身份");
    let store = AppStore::new(paths.app_db_path().to_path_buf());
    let workspace = store
        .workspace_identity_store()
        .list()
        .unwrap()
        .into_iter()
        .find(|workspace| workspace.canonical_root == project.canonicalize().unwrap())
        .expect("必须打开指定隔离项目");
    // uuidgen uses uppercase; the phone pairing protocol requires 64 lowercase hex characters.
    let token = format!("{}{}", random_id(), random_id())
        .replace('-', "")
        .to_ascii_lowercase();
    let device = random_id();
    store
        .mobile_access_store()
        .register(
            &device,
            "Real Codex L3 phone",
            &token,
            &[MobileProjectAccess {
                workspace_id: workspace.workspace_id.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("仅本隔离App控制授权");
    let endpoints: Vec<String> = identity["addresses"]
        .as_array()
        .expect("本地监听地址")
        .iter()
        .map(|address| format!("wss://{}/mobile", address.as_str().expect("监听地址文本")))
        .collect();
    let fixture = json!({"root":directory,"port":port,"machineID":identity["machine_id"],
        "namespace":random_id(),"endpoints":endpoints,
        "certificate":identity["certificate_der"],"projectID":workspace.workspace_id,"definitionID":definition.id,
        "mac":{"deviceID":device,"token":token}});
    let source = directory.join("gateway-fixture.json");
    private_json(&source, &fixture);
    let gateway_log = fs::File::create(directory.join("gateway.log")).unwrap();
    let mut gateway = Gateway(
        Command::new("node")
            .arg(repository.join("services/norma-cloud/check-real-mac-thread.mjs"))
            .arg(source)
            .env("NORMA_THREAD_CODEX_REAL_L3", "1")
            .stdout(gateway_log.try_clone().unwrap())
            .stderr(gateway_log)
            .stdin(Stdio::null())
            .spawn()
            .expect("正式本机网关"),
    );
    let phone_fixture = directory.join("phone-fixture.json");
    // 10s = local Node startup5s + invitation/claim5s; no upstream model is involved here.
    wait(Duration::from_secs(10), || {
        assert!(
            gateway.0.try_wait().unwrap().is_none(),
            "网关启动失败，查看隔离gateway.log"
        );
        phone_fixture.is_file()
    });
    eprintln!("Real Mac ready; phone will dispatch exactly one model task.");
    let swift_log = fs::File::create(directory.join("swift-real.log")).unwrap();
    let mut probe = if std::env::var("NORMA_THREAD_REAL_UI").as_deref() == Ok("1") {
        assert!(
            std::env::var_os("NORMA_IOS_SIMULATOR").is_some(),
            "原生UI验收必须显式指定模拟器"
        );
        let mut command = Command::new("bash");
        command
            .arg(repository.join("scripts/check-norma-ios-real-thread.sh"))
            .arg(&phone_fixture);
        command
    } else {
        let mut command = Command::new("swift");
        command
            .args(["test", "--package-path"])
            .arg(repository.join("ios"))
            .args([
                "--filter",
                "RealMacThreadTests/testRealCodexExecutionReturnsToOriginalThread",
            ])
            .env("NORMA_THREAD_REAL_FIXTURE", &phone_fixture);
        command
    };
    let swift = probe
        .stdout(swift_log.try_clone().unwrap())
        .stderr(swift_log)
        .status()
        .expect("真实Swift客户端验收");
    store
        .mobile_access_store()
        .revoke(&device)
        .expect("清理本轮配对授权");
    assert!(
        swift.success(),
        "真实派工验收未通过，查看 {}/swift-real.log；必须检查并退出本轮隔离App",
        directory.display()
    );
    assert_eq!(
        fs::read_to_string(project.join("result.txt")).unwrap(),
        "norma-real-codex\n",
        "文件必须由Codex实际写入"
    );
    assert_eq!(
        fs::read_to_string(project.join("executions.txt")).unwrap(),
        "executed\n",
        "真实项目不得重复执行"
    );
    eprintln!(
        "Real thread PASS; Computer Use should inspect then Quit the isolated App. Evidence: {}",
        directory.display()
    );
}

fn random_id() -> String {
    let output = Command::new("/usr/bin/uuidgen").output().expect("系统UUID");
    assert!(output.status.success(), "UUID生成必须成功");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn private_json(path: &Path, value: &Value) {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(&serde_json::to_vec(value).unwrap())
        .expect("保存隔离凭据，不输出正文");
}
fn wait(timeout: Duration, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if ready() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("本地L3等待超时；检查隔离App/日志，不能把超时当作完成");
}

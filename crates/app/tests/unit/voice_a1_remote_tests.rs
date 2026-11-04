use super::*;

struct RemoteEngine {
    bundle: PathBuf,
    requests: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    corrupt: std::sync::Arc<AtomicBool>,
    stopped: std::sync::Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl RemoteEngine {
    fn new(source: &Path, corrupt_file: Option<&'static str>) -> Self {
        use std::{
            io::Write,
            net::TcpListener,
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            },
        };
        let manifest = fs::read(source.join("engine.json")).expect("清单");
        let archive = fs::read(source.join("engine.tar.bz2")).expect("归档");
        let listener = TcpListener::bind("127.0.0.1:0").expect("HTTP夹具");
        let url = format!("http://{}", listener.local_addr().expect("地址"));
        listener.set_nonblocking(true).expect("可关闭服务");
        let requests = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let corrupt = Arc::new(AtomicBool::new(corrupt_file.is_some()));
        let bundle = source.join("small-app");
        fs::create_dir(&bundle).expect("小型资源");
        fs::write(
            bundle.join("release.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema":1, "base_url":url, "manifest_bytes":manifest.len(),
                "manifest_sha256":format!("{:x}", Sha256::digest(&manifest)),
                "minimum_macos_major":26, "target":"aarch64-apple-darwin",
                "requirements_sha256":format!("{:x}", Sha256::digest(asr::A1_REQUIREMENTS))
            }))
            .expect("描述JSON"),
        )
        .expect("下载描述");
        let (count, stop, bad) = (requests.clone(), stopped.clone(), corrupt.clone());
        let thread = std::thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                };
                stream
                    .set_nonblocking(false)
                    .expect("已接受连接须阻塞读取请求");
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .expect("请求超时");
                let mut request = [0; 4096];
                let n = stream.read(&mut request).expect("请求");
                count.fetch_add(1, Ordering::Relaxed);
                let name = if String::from_utf8_lossy(&request[..n]).contains("/engine.json ") {
                    "engine.json"
                } else {
                    "engine.tar.bz2"
                };
                let good = if name == "engine.json" {
                    &manifest
                } else {
                    &archive
                };
                let body = if bad.load(Ordering::Acquire) && corrupt_file == Some(name) {
                    vec![0; good.len()]
                } else {
                    good.clone()
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            }
        });
        Self {
            bundle,
            requests,
            stopped,
            corrupt,
            thread: Some(thread),
        }
    }
}
impl Drop for RemoteEngine {
    fn drop(&mut self) {
        self.stopped
            .store(true, std::sync::atomic::Ordering::Release);
        self.thread
            .take()
            .expect("服务线程")
            .join()
            .expect("关闭服务");
    }
}

/// 场景：安装包只有小型下载描述，没有A1环境或系统Python，仍应自动安装并离线复用。
/// 构造：真实HTTP服务提供现有归档夹具，App资源只提供固定URL和SHA256。
/// 执行：先检查本地，再显式安装，关闭服务后重开；预期本地检查零请求，安装两个请求。
/// 预期：已安装环境可离线复用，主App及安装缓存中均不保留大归档。
#[test]
fn voice_a1_remote_engine_downloads_once_and_reuses_offline() {
    use std::sync::atomic::Ordering;
    let temp = tempfile::tempdir().expect("隔离下载");
    let source = engine_fixture(temp.path());
    let remote = RemoteEngine::new(&source, None);
    let root = temp.path().join("models");
    let cancelled = AtomicBool::new(false);
    assert!(
        matches!(
            super::super::prepare_cached(&root, &remote.bundle, &cancelled),
            Err(A1InstallFailure::EngineMissing)
        ),
        "本地检查应报告尚未安装"
    );
    assert_eq!(
        remote.requests.load(Ordering::Relaxed),
        0,
        "本地检查不得隐式联网"
    );
    let python = super::super::prepare(&root, &remote.bundle, &cancelled)
        .expect("只有下载描述时应自动安装引擎");
    assert_eq!(
        remote.requests.load(Ordering::Relaxed),
        2,
        "首次只能下载可信清单和归档"
    );
    assert!(
        !remote.bundle.join("engine.tar.bz2").exists(),
        "主App不能内置环境"
    );
    let bundle = remote.bundle.clone();
    drop(remote);
    assert_eq!(
        super::super::prepare(&root, &bundle, &cancelled).expect("断网复用"),
        python,
        "已安装环境离线复用"
    );
    assert_eq!(
        super::super::prepare_cached(&root, &bundle, &cancelled).expect("离线本地检查"),
        python,
        "本地检查应复用完整环境"
    );
    let cache = root.join("a1/engine-downloads");
    for version in fs::read_dir(cache).expect("下载缓存") {
        assert!(
            !version
                .expect("版本")
                .path()
                .join("engine.tar.bz2")
                .exists(),
            "安装成功应清理可重下载的大归档"
        );
    }
}

/// 场景：网络返回同长度坏清单或归档时，不得执行或替换已有配置，恢复服务后可重试。
/// 构造：两种真实HTTP损坏响应及原配置/指针字节；可信描述始终固定正确清单。
/// 执行：显式准备，核对失败；恢复相同URL内容后重试。
/// 预期：按具体文件报告完整性错误，失败保留旧状态，重试才发布可执行环境。
#[test]
fn voice_a1_remote_engine_rejects_corruption_and_retries_without_replacing_old_state() {
    use std::sync::atomic::Ordering;
    for name in ["engine.json", "engine.tar.bz2"] {
        let temp = tempfile::tempdir().expect("损坏下载目录");
        let source = engine_fixture(temp.path());
        let remote = RemoteEngine::new(&source, Some(name));
        let root = temp.path().join("models");
        fs::create_dir_all(root.join("a1")).expect("旧配置目录");
        let pointer = root.join("a1/python-runtime.json");
        fs::write(&pointer, b"old-pointer").expect("旧指针");
        let config = root.join("a1/runtime.json");
        fs::write(&config, b"old-model-config").expect("旧模型配置");
        let result = super::super::prepare(&root, &remote.bundle, &AtomicBool::new(false));
        assert_eq!(
            result,
            Err(A1InstallFailure::Integrity(name.into())),
            "必须明确拒绝具体文件的同长度篡改"
        );
        assert_eq!(
            fs::read(&pointer).expect("旧指针"),
            b"old-pointer",
            "失败不能改写旧环境指针"
        );
        assert_eq!(
            fs::read(&config).expect("旧配置"),
            b"old-model-config",
            "失败不能丢失旧模型配置"
        );
        assert!(!root.join("a1/engines").exists(), "坏字节不能展开或执行");
        remote.corrupt.store(false, Ordering::Release);
        let python = super::super::prepare(&root, &remote.bundle, &AtomicBool::new(false))
            .expect("正确服务重试");
        assert!(
            packages_match(&python, &AtomicBool::new(false)).expect("已安装解释器"),
            "重试后必须实际可执行"
        );
        assert_eq!(
            fs::read(&config).expect("模型配置保留"),
            b"old-model-config",
            "环境升级不能重新下载模型"
        );
    }
}

/// 场景：用户在环境下载进度到达时取消，不能发布归档、解释器指针或模型配置。
/// 构造：正确HTTP资源，在实际归档进度回调中置位取消；随后重试同一描述。
/// 执行：通过安装准备入口下载并取消，核对副作用，再解除取消重试。
/// 预期：Cancelled且无受管解释器，重试可正常完成，不依赖系统Python。
#[test]
fn voice_a1_remote_engine_download_can_be_cancelled_and_retried() {
    use std::sync::atomic::Ordering;
    let temp = tempfile::tempdir().expect("取消目录");
    let source = engine_fixture(temp.path());
    let remote = RemoteEngine::new(&source, None);
    let root = temp.path().join("models");
    let cancelled = AtomicBool::new(false);
    let result =
        super::super::prepare_with_progress(&root, &remote.bundle, &cancelled, |name, _, _| {
            if name == "engine.tar.bz2" {
                cancelled.store(true, Ordering::Release);
            }
        });
    assert_eq!(
        result,
        Err(A1InstallFailure::Cancelled),
        "下载中的取消必须生效"
    );
    assert!(
        !root.join("a1/python-runtime.json").exists(),
        "取消不能发布运行环境指针"
    );
    assert!(
        !root.join("a1/runtime.json").exists(),
        "取消不能发布模型配置"
    );
    assert!(
        !root.join("a1/engines").exists(),
        "取消不能执行下载的解释器"
    );
    cancelled.store(false, Ordering::Release);
    assert!(
        super::super::prepare(&root, &remote.bundle, &cancelled).is_ok(),
        "取消后必须能重试完整安装"
    );
}

/// 场景：旧版随包安装的受管环境没有engine-downloads缓存，升级后也必须能离线检查。
/// 构造：通过旧安装入口形成完整环境，新App保留同一可信清单及小型下载描述。
/// 执行：关闭HTTP后调用只读网络策略的准备入口。
/// 预期：完整文件校验后复用旧解释器，不下载、不更换指针、不要求重新安装模型。
#[test]
fn voice_a1_previous_bundled_installation_remains_available_offline() {
    let temp = tempfile::tempdir().expect("旧版升级目录");
    let source = engine_fixture(temp.path());
    let root = temp.path().join("models");
    let cancelled = AtomicBool::new(false);
    let previous = prepare(&root, &source, &cancelled).expect("旧版随包安装");
    let before = fs::read(root.join("a1/python-runtime.json")).expect("旧指针");
    let remote = RemoteEngine::new(&source, None);
    let mut compressed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(
        &mut compressed,
        &fs::read(source.join("engine.json")).expect("清单原字节"),
    )
    .expect("无损压缩");
    fs::write(
        remote.bundle.join("engine.json.gz"),
        compressed.finish().expect("压缩完成"),
    )
    .expect("新App可信清单");
    let bundle = remote.bundle.clone();
    drop(remote);
    let actual = super::super::prepare_cached(&root, &bundle, &cancelled)
        .expect("升级后应离线复用旧完整环境");
    assert_eq!(actual, previous, "不能为完整旧安装更换解释器或重复安装");
    assert_eq!(
        fs::read(root.join("a1/python-runtime.json")).expect("旧指针"),
        before,
        "离线检查不能改写旧指针"
    );
}

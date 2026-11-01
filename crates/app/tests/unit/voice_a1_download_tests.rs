use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::atomic::AtomicBool,
};

fn server(body: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("本机HTTP夹具");
    let url = format!("http://{}/weights", listener.local_addr().expect("地址"));
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("HTTP连接");
        let mut request = [0u8; 4096];
        stream.read(&mut request).expect("读取请求");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("头");
        stream.write_all(body).expect("响应");
    });
    (url, thread)
}

/// 场景：真实HTTP下载的内容损坏必须报明确错误，重试好内容才形成可用下载文件。
/// 构造：本机TCP服务分别返回3字节bad和abc，可信清单使用abc的公开SHA256向量。
/// 执行：调用下载接口，检查失败和文件缺失；换服务重试，再读取文件与下载进度。
/// 预期：坏响应得到Integrity错误，不留下成功文件；重试得到abc且最终进度恰好3/3。
#[test]
fn voice_a1_download_rejects_corrupt_http_and_retry_succeeds() {
    let temp = tempfile::tempdir().expect("下载目录");
    let file = model_store::VerifiedFile::new(
        "weights",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    let cancelled = AtomicBool::new(false);
    let (url, thread) = server(b"bad");
    let failed = download_verified_file(&url, &file, temp.path(), &cancelled, |_, _| {});
    thread.join().expect("HTTP完成");
    assert!(
        matches!(failed, Err(A1InstallFailure::Integrity(_))),
        "损坏下载必须报告完整性错误"
    );
    assert!(
        !temp.path().join("weights").exists(),
        "坏响应不能形成成功文件"
    );
    let (url, thread) = server(b"abc");
    let mut progress = Vec::new();
    let path = download_verified_file(&url, &file, temp.path(), &cancelled, |done, total| {
        progress.push((done, total))
    })
    .expect("正确HTTP重试");
    thread.join().expect("HTTP完成");
    assert_eq!(
        std::fs::read(path).expect("下载内容"),
        b"abc",
        "必须保存实际已校验响应"
    );
    assert_eq!(
        progress.last(),
        Some(&(3, 3)),
        "下载成功必须发布实际完整进度"
    );
}

/// 场景：用户选择的Python不存在时，安装须在任何模型下载或配置替换前失败。
/// 构造：隔离模型根保存旧manifest，Python路径指向该目录下不存在的绝对路径。
/// 执行：调用完整安装入口，随后检查错误、旧manifest与包目录。
/// 预期：明确报告Python错误；原配置保持逐字不变，不能形成新模型安装。
#[test]
fn voice_a1_install_invalid_python_preserves_previous_manifest() {
    let temp = tempfile::tempdir().expect("隔离目录");
    std::fs::create_dir(temp.path().join("a1")).expect("配置目录");
    let manifest = temp.path().join("a1/runtime.json");
    std::fs::write(&manifest, b"previous-installation").expect("原配置");
    let result = install_a1(
        temp.path(),
        &temp.path().join("missing-python"),
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(
        matches!(result, Err(A1InstallFailure::Python)),
        "失效Python路径须给出可恢复的明确原因"
    );
    assert_eq!(
        std::fs::read(&manifest).expect("原配置"),
        b"previous-installation",
        "运行环境失败不能改写旧配置"
    );
    assert!(
        !temp.path().join("a1/packages").exists(),
        "Python无效不能启动模型安装"
    );
}

/// 场景：模型包已校验但配置提交失败时，未引用的新包应清理；成功提交的包必须保留。
/// 构造：隔离目录中的failed/committed两份候选包，各含一个实际权重文件。
/// 执行：失败候选直接释放；成功候选先提交所有权再释放。
/// 预期：只回收本次失败候选，已提交目录与权重保持，不影响任何旧模型目录。
#[test]
fn voice_a1_package_ownership_cleans_failed_candidate_only() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let failed = temp.path().join("failed");
    let committed = temp.path().join("committed");
    std::fs::create_dir(&failed).expect("失败候选");
    std::fs::create_dir(&committed).expect("成功候选");
    std::fs::write(failed.join("weights"), b"abc").expect("失败候选内容");
    std::fs::write(committed.join("weights"), b"abc").expect("成功候选内容");
    drop(PendingPackage::new(failed.clone()));
    let mut owned = PendingPackage::new(committed.clone());
    owned.commit();
    drop(owned);
    assert!(
        !failed.exists(),
        "配置未提交的本次候选必须回收，避免失败重试耗尽空间"
    );
    assert_eq!(
        std::fs::read(committed.join("weights")).expect("已提交模型"),
        b"abc",
        "已经提交的包不能被失败清理误删"
    );
}

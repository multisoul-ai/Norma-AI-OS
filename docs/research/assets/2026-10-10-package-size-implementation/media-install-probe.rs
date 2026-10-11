use media_store::{MediaArtifact, MediaError, MediaStore};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

struct ObservedReader<'a> {
    file: File,
    cancelled: &'a AtomicBool,
    cancel_after: Option<u64>,
    received: u64,
    max_buffer: usize,
}

impl Read for ObservedReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.max_buffer = self.max_buffer.max(buffer.len());
        let count = self.file.read(buffer)?;
        self.received += count as u64;
        if self.cancel_after.is_some_and(|limit| self.received >= limit) {
            self.cancelled.store(true, Ordering::Release);
        }
        Ok(count)
    }
}

// 场景：使用清单中最大的真实原媒体，逐次创建隔离、空白缓存目录。
// 数据为调用者传入的固定原路径/长度/SHA，执行安装、逐字节对照和离线验证。
// repair 模式损坏一个字节后修复；cancel 模式读取到 1 MiB 后取消。
// 预期成功路径只保留原文件，损坏缓存不能 Ready，取消不能提升或留下 partial。
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 7, "probe requires source, cache, relative path, length, SHA and mode");
    let source = PathBuf::from(&args[1]);
    let root = PathBuf::from(&args[2]);
    let bytes: u64 = args[4].parse().expect("manifest byte length must be an integer");
    let mode = args[6].as_str();
    assert!(matches!(mode, "install" | "repair" | "cancel"), "only the three defined probe modes are supported");
    assert!(!root.exists(), "every measured run must start without an existing cache");
    assert_eq!(fs::metadata(&source).expect("real source must exist").len(), bytes, "source length must match the pinned manifest");
    let artifact = MediaArtifact::new(&args[3], "https://github.com/multisoul-ai/norma-os-releases/releases/download/media-v1/probe.mp4", bytes, &args[5])
        .expect("actual pinned media must satisfy the public artifact contract");
    let store = MediaStore::new(root.clone());
    let cancelled = AtomicBool::new(false);
    let mut reader = ObservedReader {
        file: File::open(&source).expect("source must remain readable"),
        cancelled: &cancelled,
        cancel_after: (mode == "cancel").then_some(1024 * 1024),
        received: 0,
        max_buffer: 0,
    };
    let started = Instant::now();
    let result = store.install(&artifact, &mut reader, &cancelled);
    let install_seconds = started.elapsed().as_secs_f64();
    assert!(reader.max_buffer <= 64 * 1024, "the public installer must request at most its bounded 64 KiB buffer");
    if mode == "cancel" {
        assert!(matches!(result, Err(MediaError::Cancelled)), "cancel after 1 MiB must return the explicit cancellation error");
        assert_eq!(reader.received, 1024 * 1024, "cancellation must stop after exactly sixteen 64 KiB reads");
        assert_eq!(store.find_verified(&artifact).expect("cancelled cache query must succeed"), None, "cancelled content must never become Ready");
        assert_eq!(fs::read_dir(&root).expect("cancel cache must be readable").count(), 0, "cancel must clean the partial file");
        println!("{{\"mode\":\"cancel\",\"source_bytes\":{bytes},\"received_bytes\":{},\"install_seconds\":{install_seconds},\"max_read_buffer_bytes\":{},\"remaining_files\":0}}", reader.received, reader.max_buffer);
        return;
    }
    let path = result.expect("an actual original video must install successfully");
    assert_eq!(reader.received, bytes, "the first installation must read the whole original video");
    let mut repair_seconds = 0.0;
    let mut repair_received_bytes = 0;
    if mode == "repair" {
        let original_first = {
            let mut first = [0];
            File::open(&source).expect("source must be readable").read_exact(&mut first).expect("video must contain a first byte");
            first[0]
        };
        OpenOptions::new().write(true).open(&path).expect("isolated installed fixture must be writable")
            .write_all(&[original_first ^ 1]).expect("same-length corruption must be written");
        assert!(matches!(store.find_verified(&artifact), Err(MediaError::HashMismatch)), "single-byte corruption must not pass SHA verification");
        let mut repair_reader = ObservedReader {
            file: File::open(&source).expect("repair source must remain readable"),
            cancelled: &cancelled,
            cancel_after: None,
            received: 0,
            max_buffer: 0,
        };
        let started = Instant::now();
        let repaired = store.install(&artifact, &mut repair_reader, &cancelled)
            .expect("only original video bytes may repair the corrupted cache");
        repair_seconds = started.elapsed().as_secs_f64();
        repair_received_bytes = repair_reader.received;
        assert_eq!(repair_received_bytes, bytes, "repair must stream the complete original video again");
        assert!(repair_reader.max_buffer <= 64 * 1024, "repair must also request only bounded 64 KiB reads");
        reader.max_buffer = reader.max_buffer.max(repair_reader.max_buffer);
        assert_eq!(repaired, path, "repair must preserve the content-addressed cache identity");
    }
    let started = Instant::now();
    assert_eq!(store.find_verified(&artifact).expect("offline verification must succeed"), Some(path.clone()), "only the exact pinned video is available offline");
    let verify_seconds = started.elapsed().as_secs_f64();
    let mut original = File::open(&source).expect("original comparison file must open");
    let mut installed = File::open(&path).expect("installed comparison file must open");
    let mut left = [0; 64 * 1024];
    let mut right = [0; 64 * 1024];
    let mut compared = 0;
    loop {
        let count = original.read(&mut left).expect("original comparison read must succeed");
        if count == 0 { break; }
        installed.read_exact(&mut right[..count]).expect("installed file must contain every original byte");
        assert_eq!(&left[..count], &right[..count], "installed file must match original bytes at every offset");
        compared += count as u64;
    }
    assert_eq!(installed.read(&mut right).expect("installed EOF read must succeed"), 0, "installed file must contain no extra trailing bytes");
    assert_eq!(compared, bytes, "byte comparison must cover the whole original video");
    assert_eq!(fs::metadata(&path).expect("installed metadata must exist").len(), bytes, "installed length must preserve the exact original size");
    assert_eq!(fs::read_dir(&root).expect("cache must be readable").count(), 1, "install or repair must leave one complete file and no partial");
    println!("{{\"mode\":\"{mode}\",\"source_bytes\":{bytes},\"received_bytes\":{},\"repair_received_bytes\":{repair_received_bytes},\"install_seconds\":{install_seconds},\"repair_seconds\":{repair_seconds},\"verify_seconds\":{verify_seconds},\"max_read_buffer_bytes\":{},\"byte_identical\":true,\"remaining_files\":1}}", reader.received, reader.max_buffer);
}

use media_store::{MediaArtifact, MediaStore};
use std::io::Cursor;
use std::sync::atomic::AtomicBool;

// 场景：将已认证 catalog 对应的原始媒体安装到本地受管目录。
// 数据为标准 SHA-256 向量 abc，长度 3 字节；通过公开安装接口流式写入。
// 预期读取原字节一致，后续查询返回同一路径，目录不遗留 partial 文件。
#[test]
fn installed_media_retains_original_bytes_and_is_verified_available() {
    let root = tempfile::tempdir().expect("test cache directory must be created");
    let artifact = MediaArtifact::new(
        "wallpapers/sample.mp4",
        "https://github.com/example/releases/download/media-v1/sample.mp4",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
    .expect("the known abc vector must be a valid artifact");
    let store = MediaStore::new(root.path().to_path_buf());
    let path = store
        .install(&artifact, &mut Cursor::new(b"abc"), &AtomicBool::new(false))
        .expect("the exact original media must install");
    assert_eq!(
        std::fs::read(&path).expect("installed media must be readable"),
        b"abc",
        "installed media must match every original byte"
    );
    assert_eq!(
        store
            .find_verified(&artifact)
            .expect("validation must complete"),
        Some(path),
        "only verified media must be reported available"
    );
    assert_eq!(
        std::fs::read_dir(root.path())
            .expect("cache root must remain readable")
            .count(),
        1,
        "successful installation must leave only one complete file"
    );
}

// 场景：网络返回同长度损坏媒体或不完整媒体。仍使用 abc 的可信哈希。
// 分别安装 abd、ab、abcd；预期全部失败、缓存不可用且临时文件清理。
#[test]
fn corrupt_truncated_and_oversized_media_never_become_available() {
    for received in [b"abd".as_slice(), b"ab".as_slice(), b"abcd".as_slice()] {
        let root = tempfile::tempdir().expect("negative cache fixture must be created");
        let artifact = abc_artifact();
        let store = MediaStore::new(root.path().to_path_buf());
        assert!(
            store
                .install(
                    &artifact,
                    &mut Cursor::new(received),
                    &AtomicBool::new(false)
                )
                .is_err(),
            "invalid original bytes must fail installation: {received:?}"
        );
        assert_eq!(
            store
                .find_verified(&artifact)
                .expect("missing cache query must succeed"),
            None,
            "invalid bytes must not be published as ready"
        );
        assert_eq!(
            std::fs::read_dir(root.path())
                .expect("cache fixture must remain readable")
                .count(),
            0,
            "failed installation must clean temporary files"
        );
    }
}

// 场景：用户在下载期间取消。Reader 在第一块返回原字节后设置取消信号。
// 执行安装；预期即使已收到正确内容，也不提升最终文件或报告 Ready。
#[test]
fn cancellation_after_reading_original_bytes_prevents_publication() {
    struct CancellingReader<'a> {
        inner: Cursor<&'static [u8]>,
        flag: &'a AtomicBool,
    }
    impl std::io::Read for CancellingReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let count = self.inner.read(buffer)?;
            self.flag.store(true, std::sync::atomic::Ordering::Release);
            Ok(count)
        }
    }
    let root = tempfile::tempdir().expect("cancel fixture directory must be created");
    let cancelled = AtomicBool::new(false);
    let mut reader = CancellingReader {
        inner: Cursor::new(b"abc"),
        flag: &cancelled,
    };
    let store = MediaStore::new(root.path().to_path_buf());
    assert!(
        matches!(
            store.install(&abc_artifact(), &mut reader, &cancelled),
            Err(media_store::MediaError::Cancelled)
        ),
        "cancelled download must return the explicit cancellation state"
    );
    assert_eq!(
        store
            .find_verified(&abc_artifact())
            .expect("cancelled cache lookup must succeed"),
        None,
        "cancelled content must not be made available"
    );
    assert_eq!(
        std::fs::read_dir(root.path())
            .expect("cancelled cache root must be readable")
            .count(),
        0,
        "cancelled installation must leave no partial file"
    );
}

// 场景：原缓存文件被损坏。先按公开接口安装 abc，再将缓存改成 abd。
// 预期验证拒绝损坏缓存，重新安装原文件后恢复完整且仍只有一份文件。
#[test]
fn invalid_existing_cache_can_be_repaired_only_with_verified_bytes() {
    let root = tempfile::tempdir().expect("repair fixture must be created");
    let artifact = abc_artifact();
    let store = MediaStore::new(root.path().to_path_buf());
    let path = store
        .install(&artifact, &mut Cursor::new(b"abc"), &AtomicBool::new(false))
        .expect("initial original bytes must install");
    std::fs::write(&path, b"abd").expect("cache corruption fixture must be written");
    assert!(
        matches!(
            store.find_verified(&artifact),
            Err(media_store::MediaError::HashMismatch)
        ),
        "same-length corrupted cache must not pass integrity validation"
    );
    assert_eq!(
        store
            .install(&artifact, &mut Cursor::new(b"abc"), &AtomicBool::new(false))
            .expect("original bytes must repair corrupted cache"),
        path,
        "repair must preserve the content-addressed cache identity"
    );
    assert_eq!(
        std::fs::read(path).expect("repaired media must be readable"),
        b"abc",
        "repair must restore the exact original bytes"
    );
}

fn abc_artifact() -> MediaArtifact {
    MediaArtifact::new(
        "wallpapers/sample.mp4",
        "https://example.com/abc.mp4",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
    .expect("known abc metadata must remain a valid public fixture")
}

// 场景：受管缓存的父目录被替换成指向目录外的软链接。
// 构造 alias→outside 后以 alias/media 安装；预期拒绝安装和查询，outside 不出现文件。
#[cfg(unix)]
#[test]
fn symlink_cache_ancestor_cannot_redirect_installation_or_lookup() {
    let fixture = tempfile::tempdir().expect("symlink fixture must be created");
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&outside).expect("outside directory must be created");
    let alias = fixture.path().join("alias");
    std::os::unix::fs::symlink(&outside, &alias).expect("alias must point outside cache");
    let store = MediaStore::new(alias.join("media"));
    assert!(
        matches!(
            store.install(
                &abc_artifact(),
                &mut Cursor::new(b"abc"),
                &AtomicBool::new(false)
            ),
            Err(media_store::MediaError::UnsafeCachePath)
        ),
        "a cache ancestor alias must never redirect writes outside the managed root"
    );
    assert!(
        matches!(
            store.find_verified(&abc_artifact()),
            Err(media_store::MediaError::UnsafeCachePath)
        ),
        "even missing cache queries must reject an unsafe ancestor"
    );
    assert!(
        !outside.join("media").exists(),
        "rejected cache must not create outside directories"
    );
}

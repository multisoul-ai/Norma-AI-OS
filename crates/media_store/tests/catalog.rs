use media_store::MediaCatalog;

// 场景：读取随签名 App 携带的固定 catalog，保留旧内置相对资源身份。
// 构造一份 abc 原文件元数据；公开解析和查找后预期长度、哈希、URL 一致。
#[test]
fn trusted_catalog_preserves_stable_resource_identity_and_original_metadata() {
    let catalog = MediaCatalog::from_json(
        r#"{
      "schema_version": 1, "resource_version": "media-v1",
      "resources": [{"path":"wallpapers/sample.mp4", "bytes":3,
        "sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "url":"https://example.com/abc.mp4", "bundled":false}]
    }"#,
    )
    .expect("valid authenticated catalog must parse");
    let artifact = catalog
        .find("wallpapers/sample.mp4")
        .expect("legacy resource ID must remain resolvable");
    assert_eq!(artifact.bytes(), 3, "catalog must retain original length");
    assert_eq!(
        artifact.url(),
        "https://example.com/abc.mp4",
        "catalog must retain pinned download location"
    );
    assert_eq!(
        artifact.sha256(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "catalog must retain authenticated original file hash"
    );
    assert!(
        catalog.find("wallpapers/missing.mp4").is_none(),
        "unknown IDs must not invent remote assets"
    );
}

// 场景：签名 catalog 的结构/元数据也需满足本地安装约束。
// 从同一 abc 正常 fixture 分别改 schema、版本、重复项、路径、URL、长度与 SHA。
// 预期逐项明确拒绝，不能生成部分可信 catalog 或凭未知版本创建目录。
#[test]
fn malformed_catalogs_are_rejected_before_any_installation() {
    let valid = serde_json::json!({
        "schema_version": 1, "resource_version": "media-v1",
        "resources": [{"path":"wallpapers/sample.mp4", "bytes":3,
            "sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "url":"https://example.com/abc.mp4", "bundled":false}]
    });
    let mut cases = Vec::new();
    for (field, value) in [
        ("schema_version", serde_json::json!(2)),
        ("resource_version", serde_json::json!("../v1")),
        ("resources", serde_json::json!([])),
    ] {
        let mut malformed = valid.clone();
        malformed[field] = value;
        cases.push((field, malformed));
    }
    let mut duplicate = valid.clone();
    duplicate["resources"]
        .as_array_mut()
        .expect("fixture resources must be an array")
        .push(valid["resources"][0].clone());
    cases.push(("duplicate ID", duplicate));
    for (field, value) in [
        ("path", serde_json::json!("../../outside.mp4")),
        ("url", serde_json::json!("http://example.com/abc.mp4")),
        (
            "url",
            serde_json::json!("https://user:secret@example.com/abc.mp4"),
        ),
        ("bytes", serde_json::json!(-1)),
        ("sha256", serde_json::json!("invalid")),
    ] {
        let mut malformed = valid.clone();
        malformed["resources"][0][field] = value;
        cases.push((field, malformed));
    }
    for (name, malformed) in cases {
        assert!(
            MediaCatalog::from_json(&malformed.to_string()).is_err(),
            "malformed {name} must not yield a trusted catalog"
        );
    }
    assert!(
        MediaCatalog::from_json(&valid.to_string()).is_ok(),
        "negative validation must still accept original authenticated abc metadata"
    );
}

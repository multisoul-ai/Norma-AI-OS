//! Explicit QA-only transport; original signed metadata remains unchanged.

pub(super) fn transport_url(
    original: &str,
    override_base: Option<&str>,
    production: bool,
) -> Result<String, String> {
    let Some(base) = override_base else {
        return Ok(original.into());
    };
    if production {
        return Err("QA media override is unavailable with production-data".into());
    }
    let mut base = url::Url::parse(base).map_err(|error| error.to_string())?;
    if !matches!(base.scheme(), "http" | "https")
        || !matches!(base.host_str(), Some("127.0.0.1" | "[::1]"))
        || !base.username().is_empty()
        || base.password().is_some()
        || base.path() != "/"
        || base.query().is_some()
        || base.fragment().is_some()
        || base.port().is_none()
    {
        return Err("QA media source must be a literal loopback host with an explicit port".into());
    }
    let original = url::Url::parse(original).map_err(|error| error.to_string())?;
    let filename = original
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .filter(|part| !part.is_empty())
        .ok_or("missing original media file name")?;
    base.set_path(&format!("/{filename}"));
    Ok(base.to_string())
}

#[cfg(test)]
mod tests {
    use super::transport_url;

    // 场景：正常 QA 请求本机原始哈希命名文件，生产请求保持固定 URL。
    // 构造可信 HTTPS 文件 URL 和显式 loopback 服务；预期只替换传输源，不替换文件标识。
    #[test]
    fn isolated_qa_can_use_loopback_without_replacing_original_identity() {
        let original = "https://github.com/example/media-v1/abc.mp4";
        assert_eq!(
            transport_url(original, Some("http://127.0.0.1:8765"), false)
                .expect("loopback fixture must be allowed"),
            "http://127.0.0.1:8765/abc.mp4",
            "QA must request the same pinned file name"
        );
        assert_eq!(
            transport_url(original, None, true).expect("production URL must remain valid"),
            original,
            "production without override must preserve its original URL"
        );
    }

    // 场景：误把 QA 编译与 production-data 同时开启，或配置外部 HTTP 源。
    // 预期两者都拒绝，不能将测试入口变成正式媒体源绕过。
    #[test]
    fn qa_override_rejects_production_and_non_loopback_servers() {
        let original = "https://example.com/abc.mp4";
        for (base, production) in [
            ("http://127.0.0.1:8765", true),
            ("http://example.com", false),
            ("http://localhost:8765", false),
            ("http://127.0.0.1:8765/untrusted", false),
        ] {
            assert!(
                transport_url(original, Some(base), production).is_err(),
                "QA override {base} with production={production} must be rejected"
            );
        }
    }
}

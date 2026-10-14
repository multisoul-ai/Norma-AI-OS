use norma::orchestration::{installed_managed_root, InstallPaths};
use std::path::Path;

/// 已安装的 CLI 位于 `<root>/bin/norma`：从自身位置推导管理目录，Test 安装读 Test 凭据；
/// App 包内二进制（Contents/MacOS）不改变默认管理目录。
#[test]
fn installed_cli_derives_managed_root_from_its_bin_dir() {
    let test_bin = Path::new("/Users/t/Library/Application Support/Norma OS Test/bin");
    assert_eq!(
        installed_managed_root(test_bin).as_deref(),
        Some(Path::new(
            "/Users/t/Library/Application Support/Norma OS Test"
        ))
    );
    assert_eq!(
        installed_managed_root(Path::new("/Applications/Norma.app/Contents/MacOS")),
        None
    );
    let paths = InstallPaths::for_home_and_bundle_bin(Path::new("/Users/t"), test_bin)
        .with_managed_root(installed_managed_root(test_bin).unwrap());
    assert_eq!(
        paths.credential_path(norma::orchestration::OrchestrationProvider::Codex),
        Path::new("/Users/t/Library/Application Support/Norma OS Test/orchestration/codex.json")
    );
}

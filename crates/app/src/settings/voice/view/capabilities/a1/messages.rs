use crate::settings::voice::a1_install::A1InstallFailure;

pub(in crate::settings::voice::view) fn failure(error: &A1InstallFailure) -> String {
    let key = match error {
        A1InstallFailure::Cancelled => "voice.a1_install_cancelled",
        A1InstallFailure::Python => "voice.a1_invalid_python",
        A1InstallFailure::PythonVersion => "voice.a1_python_version",
        A1InstallFailure::Dependencies => "voice.a1_dependencies_failed",
        A1InstallFailure::DependencyVersions => "voice.a1_dependency_versions",
        A1InstallFailure::Manifest => "voice.a1_manifest_failed",
        A1InstallFailure::Network(file) => {
            return ui::i18n::format("voice.a1_download_network", &[("file", file.clone())])
        }
        A1InstallFailure::Http { file, status } => {
            return ui::i18n::format(
                "voice.a1_download_http",
                &[("file", file.clone()), ("status", status.to_string())],
            )
        }
        A1InstallFailure::Integrity(file) => {
            return ui::i18n::format("voice.a1_download_integrity", &[("file", file.clone())])
        }
        A1InstallFailure::Disk { file, code } => {
            return ui::i18n::format(
                "voice.a1_install_disk",
                &[
                    ("file", file.clone()),
                    (
                        "code",
                        code.map(|c| c.to_string()).unwrap_or_else(|| "IO".into()),
                    ),
                ],
            )
        }
    };
    ui::i18n::text(key).to_string()
}

pub(in crate::settings::voice::view) fn preparation_failure(error: &str) -> String {
    ui::i18n::text(if error.contains("memory pressure") {
        "voice.a1_memory_pressure"
    } else if error.contains("No such file") || error.contains("not found") {
        "voice.a1_missing_files"
    } else if error.contains("integrity") {
        "voice.a1_existing_integrity"
    } else {
        "voice.a1_prepare_failed"
    })
    .to_string()
}

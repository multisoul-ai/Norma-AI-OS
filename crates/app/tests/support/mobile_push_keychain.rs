//! Opt-in L3 worker: call the normal app persistence functions without booting a GPUI app or hooks.
use super::{effective, load, read, save, write, PushRelayConfig};
use anyhow::{ensure, Context, Result};
use std::path::Path;
use storage::{AppStore, PushEnvironment};

pub fn run_push_keychain_probe(database: &Path, step: &str) -> Result<()> {
    ensure!(
        !cfg!(any(test, feature = "e2e-test-support")),
        "Keychain L3 requires a normal app library without test/e2e credential substitutes"
    );
    let parent = database
        .parent()
        .context("Missing isolated database directory")?
        .canonicalize()?;
    let temporary = std::env::temp_dir().canonicalize()?;
    ensure!(
        parent.parent() == Some(temporary.as_path())
            && parent.file_name().is_some_and(|name| name
                .to_string_lossy()
                .starts_with("norma-push-keychain-l3-"))
            && matches!(
                database.file_name().and_then(|s| s.to_str()),
                Some("a.sqlite" | "b.sqlite")
            ),
        "Probe accepts only its isolated temporary AppDB"
    );
    let store = AppStore::new(database.to_path_buf());
    let account = store.mobile_access_store().keychain_account()?;
    match step {
        "absent" => {
            ensure!(
                load(&store)?.is_none(),
                "Another installation inherited a saved configuration"
            );
            let fallback = effective(&store)?.context("Environment fallback is missing")?;
            ensure!(
                fallback.endpoint.as_str() == "https://fallback.invalid/v1/notifications",
                "Wrong fallback endpoint"
            );
            ensure!(
                fallback.key == "a".repeat(64) && fallback.environment == PushEnvironment::Sandbox,
                "Wrong fallback credential/environment"
            );
        }
        "save-sandbox" | "save-production" => {
            let (url, key, environment) = expected(step == "save-production");
            save(
                &store,
                Some(&PushRelayConfig::from_fields(url, &key, environment)?),
            )?;
        }
        "read-sandbox" | "read-production" => {
            let (url, key, environment) = expected(step == "read-production");
            let config =
                effective(&store)?.context("Saved configuration disappeared after process exit")?;
            ensure!(
                config.endpoint.as_str() == url,
                "Saved endpoint was not restored"
            );
            ensure!(config.key == key, "Saved credential was not restored");
            ensure!(
                config.environment == environment,
                "Saved APNs environment was not restored"
            );
            ensure!(
                config.endpoint.as_str() != "https://fallback.invalid/v1/notifications",
                "Saved configuration must override fallback"
            );
        }
        "disable" => save(&store, None)?,
        "read-disabled" => {
            ensure!(
                matches!(load(&store)?, Some(None)),
                "Explicit disable marker was lost"
            );
            ensure!(
                effective(&store)?.is_none(),
                "Disabled configuration revived from environment"
            );
        }
        "corrupt" => write(&account, b"invalid-json")?,
        "read-corrupt" => {
            ensure!(
                effective(&store).is_err(),
                "Corrupt saved configuration silently used fallback"
            );
            ensure!(
                read(&account)?.as_deref() == Some(b"invalid-json"),
                "Failed read overwrote the damaged item"
            );
        }
        "cleanup" => {
            match security_framework::passwords::delete_generic_password(
                "com.norma-ai.os.mobile-push",
                &account,
            ) {
                Ok(()) => {}
                Err(e) if e.code() == -25300 => {}
                Err(_) => anyhow::bail!("Cannot remove isolated Keychain item"),
            }
            ensure!(
                read(&account)?.is_none(),
                "Isolated Keychain item remains after cleanup"
            );
        }
        _ => anyhow::bail!("Unknown Keychain probe step"),
    }
    Ok(())
}

fn expected(production: bool) -> (&'static str, String, PushEnvironment) {
    // 32 synthetic bytes × 2 hex chars = 64; no Apple credentials or network access.
    if production {
        (
            "https://production.invalid/v1/notifications",
            "c".repeat(64),
            PushEnvironment::Production,
        )
    } else {
        (
            "https://sandbox.invalid/v1/notifications",
            "b".repeat(64),
            PushEnvironment::Sandbox,
        )
    }
}

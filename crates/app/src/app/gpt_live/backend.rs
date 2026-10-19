//! GPT's tool backend shares only its own Live credential, with fixed official defaults.
pub(super) fn load_config(
    store: &storage::SettingsStore,
    account: &str,
) -> Result<agent::ProviderConfig, String> {
    if !account.starts_with("gpt-live-desktop-")
        || store
            .get_string("voice.gpt_live.credential_ref")
            .map_err(|error| error.to_string())?
            .as_deref()
            != Some(account)
    {
        return Err("GPT-Live Key已更改或移除，请重新连接。".into());
    }
    let secret = crate::settings::voice::VoiceSettingsState::load_credential(store, account)?
        .filter(|secret| !secret.trim().is_empty())
        .ok_or("GPT-Live API Key缺失，请重新保存。")?;
    let config = agent::ProviderConfig::new(
        agent::ProviderProtocol::OpenAiCompatible,
        "https://api.openai.com/v1",
        "gpt-5.4-mini",
        Some(secret),
    );
    config.validate().map_err(|error| error.to_string())?;
    Ok(config)
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_backend_tests.rs"]
mod tests;

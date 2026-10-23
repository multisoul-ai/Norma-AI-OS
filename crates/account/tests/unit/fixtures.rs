use super::*;

pub(crate) fn profile(provider: Provider, subject: &str) -> Profile {
    Profile {
        account_id: AccountId::new(format!("acc_{subject}")),
        display_name: "Zhou Kai".to_string(),
        email: Some("zhoukai@example.com".to_string()),
        avatar_url: None,
        identities: vec![Identity {
            provider,
            subject: subject.to_string(),
        }],
    }
}

pub(crate) fn session(expires_at: u64) -> Session {
    Session::new(
        "access-secret".to_string(),
        "refresh-secret".to_string(),
        expires_at,
        profile(Provider::GitHub, "42"),
    )
}

use super::Keychain;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, PartialEq, Eq)]
pub struct TokenHash {
    pub hash: [u8; 32],
    pub expires_at: Option<i64>,
}
pub trait CredentialStore: Send + Sync {
    fn tokens(&self) -> anyhow::Result<Vec<TokenHash>>;
    fn replace_tokens(&self, tokens: &[TokenHash]) -> anyhow::Result<()>;
    fn audit_auth_failure(
        &self,
        now: i64,
        source: IpAddr,
        window_start: i64,
        count: u64,
        locked_until: i64,
    ) -> anyhow::Result<()>;
    fn audit(&self, now: i64, tool: &str, parameters: &str, result: &str) -> anyhow::Result<()>;
}
use subtle::ConstantTimeEq;
use tokio_util::sync::CancellationToken;

pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }
}
#[derive(Clone)]
pub struct Authenticated(pub(crate) [u8; 32]);
struct FailureWindow {
    started: i64,
    count: u64,
    locked_until: i64,
}
const FAILURE_WINDOW: i64 = 15 * 60;
struct State {
    tokens: Vec<TokenHash>,
    failures: HashMap<IpAddr, FailureWindow>,
    reads: HashMap<[u8; 32], (i64, u32)>,
    sessions: CancellationToken,
}
pub struct Auth {
    store: Arc<dyn CredentialStore>,
    keychain: Arc<dyn Keychain>,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
}
impl Auth {
    /// Background-only: database and Keychain operations must not run on GPUI.
    pub fn load(
        store: Arc<dyn CredentialStore>,
        keychain: Arc<dyn Keychain>,
        clock: Arc<dyn Clock>,
    ) -> anyhow::Result<Self> {
        let mut tokens = store.tokens()?;
        let mut valid = Vec::new();
        for token in tokens.drain(..) {
            if let Some(value) = keychain.get(&account(&token.hash))? {
                let digest: [u8; 32] = Sha256::digest(value).into();
                if bool::from(digest.ct_eq(&token.hash))
                    && token.expires_at.is_none_or(|t| t > clock.now())
                {
                    valid.push(token);
                } else {
                    keychain.delete(&account(&token.hash))?;
                }
            }
        }
        store.replace_tokens(&valid)?;
        Ok(Self {
            store,
            keychain,
            clock,
            state: Mutex::new(State {
                tokens: valid,
                failures: HashMap::new(),
                reads: HashMap::new(),
                sessions: CancellationToken::new(),
            }),
        })
    }
    pub fn now(&self) -> i64 {
        self.clock.now()
    }
    /// Returns plaintext exactly once. The UI must erase it when dismissed or navigated away.
    pub fn generate(&self) -> anyhow::Result<String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("auth lock"))?;
        let secret = format!("nrm_live_{}", URL_SAFE_NO_PAD.encode(random_bytes()?));
        let hash: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
        self.keychain.set(&account(&hash), secret.as_bytes())?;
        let mut tokens = Vec::new();
        for token in &state.tokens {
            if token.expires_at.is_some_and(|t| t <= self.now()) {
                self.keychain.delete(&account(&token.hash))?;
            } else {
                tokens.push(TokenHash {
                    hash: token.hash,
                    expires_at: Some(token.expires_at.unwrap_or(self.now() + 86400)),
                });
            }
        }
        tokens.push(TokenHash {
            hash,
            expires_at: None,
        });
        if let Err(error) = self.store.replace_tokens(&tokens) {
            let _ = self.keychain.delete(&account(&hash));
            return Err(error);
        }
        state.tokens = tokens;
        Ok(secret)
    }
    pub fn revoke(&self) -> anyhow::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("auth lock"))?;
        // Deny first even if deleting a Keychain item fails. Always disconnect existing sessions.
        let tokens = std::mem::take(&mut state.tokens);
        state.sessions.cancel();
        state.sessions = CancellationToken::new();
        state.reads.clear();
        self.store.replace_tokens(&[])?;
        let mut result = Ok(());
        for token in tokens {
            if let Err(error) = self.keychain.delete(&account(&token.hash)) {
                result = Err(error);
            }
        }
        result
    }
    pub fn sessions(&self) -> CancellationToken {
        self.state.lock().expect("auth lock").sessions.clone()
    }
    pub fn token_status(&self) -> Vec<Option<i64>> {
        self.state
            .lock()
            .expect("auth lock")
            .tokens
            .iter()
            .filter(|t| t.expires_at.is_none_or(|e| e > self.now()))
            .map(|t| t.expires_at)
            .collect()
    }
    pub fn locked_until(&self) -> i64 {
        self.state
            .lock()
            .expect("auth lock")
            .failures
            .values()
            .map(|window| window.locked_until)
            .filter(|until| *until > self.now())
            .max()
            .unwrap_or(0)
    }
    pub fn locked_until_for(&self, source: IpAddr) -> i64 {
        self.state
            .lock()
            .expect("auth lock")
            .failures
            .get(&source)
            .map(|window| window.locked_until)
            .filter(|until| *until > self.now())
            .unwrap_or(0)
    }
    pub fn authenticate(&self, token: Option<&str>) -> Result<Authenticated, &'static str> {
        self.authenticate_from(Ipv4Addr::LOCALHOST.into(), token)
    }
    /// Background-only. Valid credentials bypass source lockout and do not reset its window.
    pub fn authenticate_from(
        &self,
        source: IpAddr,
        token: Option<&str>,
    ) -> Result<Authenticated, &'static str> {
        let mut state = self.state.lock().map_err(|_| "internal")?;
        let now = self.now();
        let digest: [u8; 32] = Sha256::digest(token.unwrap_or("").as_bytes()).into();
        let mut accepted = false;
        for known in &state.tokens {
            accepted |=
                bool::from(known.hash.ct_eq(&digest)) && known.expires_at.is_none_or(|t| t > now);
        }
        if token.is_some() && accepted {
            return Ok(Authenticated(digest));
        }
        state
            .failures
            .retain(|_, window| now < window.started + FAILURE_WINDOW || now < window.locked_until);
        let window = state.failures.entry(source).or_insert(FailureWindow {
            started: now,
            count: 0,
            locked_until: 0,
        });
        window.count = window.count.saturating_add(1);
        if now >= window.locked_until && window.count >= 10 {
            window.locked_until = now + FAILURE_WINDOW;
        }
        self.store
            .audit_auth_failure(
                now,
                source,
                window.started,
                window.count,
                window.locked_until,
            )
            .map_err(|_| "internal")?;
        Err("unauthorized")
    }
    /// Recheck under the same lock as revocation and rate accounting; sessions cannot cache auth.
    pub fn read(&self, identity: &Authenticated) -> Result<(), (&'static str, Option<i64>)> {
        let mut state = self.state.lock().map_err(|_| ("internal", None))?;
        let now = self.now();
        if !state
            .tokens
            .iter()
            .any(|t| bool::from(t.hash.ct_eq(&identity.0)) && t.expires_at.is_none_or(|e| e > now))
        {
            return Err(("unauthorized", None));
        }
        let entry = state.reads.entry(identity.0).or_insert((now, 0));
        if now >= entry.0 + 60 {
            *entry = (now, 0);
        }
        if entry.1 >= 60 {
            return Err(("rate_limited", Some((entry.0 + 60 - now).max(1))));
        }
        entry.1 += 1;
        Ok(())
    }
    pub fn audit(&self, tool: &str, result: &str) -> anyhow::Result<()> {
        self.store.audit(self.now(), tool, "", result)?;
        Ok(())
    }
    pub fn audit_parameters(
        &self,
        tool: &str,
        parameters: &str,
        result: &str,
    ) -> anyhow::Result<()> {
        self.store.audit(self.now(), tool, parameters, result)
    }
    pub fn signer_key(&self) -> anyhow::Result<Vec<u8>> {
        if let Some(key) = self.keychain.get("card-identity-hmac")? {
            anyhow::ensure!(key.len() == 32, "invalid identity key");
            return Ok(key);
        }
        let key = random_bytes()?;
        self.keychain.set("card-identity-hmac", &key)?;
        Ok(key.into())
    }
}
fn account(hash: &[u8; 32]) -> String {
    format!("token-{}", hex::encode(hash))
}
fn random_bytes() -> anyhow::Result<[u8; 32]> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("secure random unavailable"))?;
    Ok(bytes)
}

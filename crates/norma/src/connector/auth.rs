use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
pub use workspace::connector_credentials::TokenRecord;
struct TokenHash {
    hash: [u8; 32],
    expires_at: Option<i64>,
}
pub trait CredentialStore: Send + Sync {
    fn tokens(&self) -> anyhow::Result<Vec<TokenRecord>>;
    fn replace_tokens(&self, tokens: &[TokenRecord]) -> anyhow::Result<()>;
    fn secret_or_insert(&self, name: &str, candidate: &[u8], now: i64) -> anyhow::Result<Vec<u8>>;
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
pub struct Authenticated(pub(crate) [u8; 32], String);
impl Authenticated {
    pub fn client_epoch(&self) -> &str {
        &self.1
    }
}
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
    writes: HashMap<[u8; 32], (i64, u32)>,
    epoch: String,
    sessions: CancellationToken,
}
pub struct Auth {
    store: Arc<dyn CredentialStore>,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
}
impl Auth {
    /// Background-only: all credential persistence uses the injected SQLite adapter.
    pub fn load(store: Arc<dyn CredentialStore>, clock: Arc<dyn Clock>) -> anyhow::Result<Self> {
        let tokens = store.tokens()?;
        let valid: Vec<_> = tokens
            .into_iter()
            .filter(|t| t.expires_at.is_none_or(|until| until > clock.now()))
            .collect();
        store.replace_tokens(&valid)?;
        let epoch = match valid.iter().max_by_key(|t| t.created_at) {
            Some(token) => hex::encode(store.secret_or_insert(
                &epoch_key(&token.token),
                &random_bytes()?,
                clock.now(),
            )?),
            None => String::new(),
        };
        Ok(Self {
            store,
            clock,
            state: Mutex::new(State {
                tokens: hashes(&valid),
                failures: HashMap::new(),
                reads: HashMap::new(),
                writes: HashMap::new(),
                epoch,
                sessions: CancellationToken::new(),
            }),
        })
    }
    pub fn now(&self) -> i64 {
        self.clock.now()
    }
    /// Persist the current token and retain every unexpired old token for its existing grace period.
    pub fn generate(&self) -> anyhow::Result<String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("auth lock"))?;
        let secret = format!("nrm_live_{}", URL_SAFE_NO_PAD.encode(random_bytes()?));
        let now = self.now();
        let mut tokens = self.store.tokens()?;
        tokens.retain(|t| t.expires_at.is_none_or(|until| until > now));
        for token in &mut tokens {
            token.expires_at = Some(token.expires_at.unwrap_or(now + 86400));
        }
        let candidate = if state.epoch.is_empty() {
            random_bytes()?.to_vec()
        } else {
            hex::decode(&state.epoch)?
        };
        let epoch = self
            .store
            .secret_or_insert(&epoch_key(&secret), &candidate, now)?;
        tokens.push(TokenRecord {
            token: secret.clone(),
            created_at: now,
            expires_at: None,
        });
        self.store.replace_tokens(&tokens)?;
        state.tokens = hashes(&tokens);
        state.epoch = hex::encode(epoch);
        Ok(secret)
    }
    pub fn revoke(&self) -> anyhow::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("auth lock"))?;
        // Deny first and disconnect existing sessions even if persistence fails.
        state.tokens.clear();
        state.sessions.cancel();
        state.sessions = CancellationToken::new();
        state.reads.clear();
        state.writes.clear();
        state.epoch.clear();
        self.store.replace_tokens(&[])
    }
    /// Background-only. Only credentials still accepted by this Auth instance are displayable.
    pub fn visible_tokens(&self) -> anyhow::Result<Vec<TokenRecord>> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("auth lock"))?;
        let now = self.now();
        Ok(self
            .store
            .tokens()?
            .into_iter()
            .filter(|token| {
                let digest: [u8; 32] = Sha256::digest(token.token.as_bytes()).into();
                state.tokens.iter().any(|known| {
                    bool::from(known.hash.ct_eq(&digest))
                        && known.expires_at.is_none_or(|until| until > now)
                })
            })
            .collect())
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
    pub fn authenticate(
        &self,
        token: Option<&str>,
    ) -> Result<Authenticated, (&'static str, Option<i64>)> {
        self.authenticate_from(Ipv4Addr::LOCALHOST.into(), token)
    }
    /// Background-only. Valid credentials bypass source lockout and do not reset its window.
    pub fn authenticate_from(
        &self,
        source: IpAddr,
        token: Option<&str>,
    ) -> Result<Authenticated, (&'static str, Option<i64>)> {
        let mut state = self.state.lock().map_err(|_| ("internal", None))?;
        let now = self.now();
        let digest: [u8; 32] = Sha256::digest(token.unwrap_or("").as_bytes()).into();
        let mut accepted = false;
        for known in &state.tokens {
            accepted |=
                bool::from(known.hash.ct_eq(&digest)) && known.expires_at.is_none_or(|t| t > now);
        }
        if token.is_some() && accepted {
            return Ok(Authenticated(digest, state.epoch.clone()));
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
            .map_err(|_| ("internal", None))?;
        if window.locked_until > now {
            Err(("rate_limited", Some(window.locked_until - now)))
        } else {
            Err(("unauthorized", None))
        }
    }
    /// Recheck under the same lock as revocation and rate accounting; sessions cannot cache auth.
    pub fn read(&self, identity: &Authenticated) -> Result<(), (&'static str, Option<i64>)> {
        self.limit(identity, false)
    }
    pub fn limit(
        &self,
        identity: &Authenticated,
        write: bool,
    ) -> Result<(), (&'static str, Option<i64>)> {
        let mut state = self.state.lock().map_err(|_| ("internal", None))?;
        let now = self.now();
        if !state
            .tokens
            .iter()
            .any(|t| bool::from(t.hash.ct_eq(&identity.0)) && t.expires_at.is_none_or(|e| e > now))
        {
            return Err(("unauthorized", None));
        }
        let budget = if write { 10 } else { 60 };
        let counts = if write {
            &mut state.writes
        } else {
            &mut state.reads
        };
        let entry = counts.entry(identity.0).or_insert((now, 0));
        if now >= entry.0 + 60 {
            *entry = (now, 0);
        }
        if entry.1 >= budget {
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
        self.secret_key("card-identity-hmac")
    }
    pub fn webhook_key(&self) -> anyhow::Result<Vec<u8>> {
        self.secret_key("webhook-hmac")
    }
    fn secret_key(&self, name: &str) -> anyhow::Result<Vec<u8>> {
        let key = self
            .store
            .secret_or_insert(name, &random_bytes()?, self.now())?;
        anyhow::ensure!(key.len() == 32, "invalid connector key");
        Ok(key)
    }
}
fn hashes(tokens: &[TokenRecord]) -> Vec<TokenHash> {
    tokens
        .iter()
        .map(|token| TokenHash {
            hash: Sha256::digest(token.token.as_bytes()).into(),
            expires_at: token.expires_at,
        })
        .collect()
}
fn random_bytes() -> anyhow::Result<[u8; 32]> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("secure random unavailable"))?;
    Ok(bytes)
}

fn epoch_key(token: &str) -> String {
    format!(
        "client-epoch:{}",
        hex::encode(Sha256::digest(token.as_bytes()))
    )
}

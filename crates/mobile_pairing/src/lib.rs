//! Short-code bootstrap shared by macOS and iOS. Contains no network or app dependencies.
use anyhow::{anyhow, ensure, Result};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use spake2::{Ed25519Group, Identity, Password, Spake2};

mod ffi;

// Protocol-specific identities separate this exchange from every other SPAKE2 use.
const PHONE: &[u8] = b"norma-pair-code-v1/phone";
const DESKTOP: &[u8] = b"norma-pair-code-v1/desktop";
const CONTEXT: &[u8] = b"norma-pair-code-v1/offer";
pub const MESSAGE_BYTES: usize = 33;
pub const MAX_OFFER_BYTES: usize = 4096;
pub const MAX_SEALED_BYTES: usize = MAX_OFFER_BYTES + 12 + 16; // nonce + GCM tag

pub struct Client(Spake2<Ed25519Group>);
pub struct Reply {
    pub message: Vec<u8>,
    pub sealed: Vec<u8>,
}

impl Client {
    pub fn start(code: &str) -> Result<(Self, Vec<u8>)> {
        validate(code)?;
        let (state, message) = Spake2::start_a(
            &Password::new(code.as_bytes()),
            &Identity::new(PHONE),
            &Identity::new(DESKTOP),
        );
        Ok((Self(state), message))
    }

    /// Consumes this exchange; authenticated plaintext is the only successful output.
    pub fn finish(self, message: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            message.len() == MESSAGE_BYTES && (28..=MAX_SEALED_BYTES).contains(&sealed.len()),
            "invalid pairing response"
        );
        let key = self
            .0
            .finish(message)
            .map_err(|_| anyhow!("invalid pairing message"))?;
        let nonce: [u8; 12] = sealed[..12].try_into()?;
        let mut ciphertext = sealed[12..].to_vec();
        let data = cipher(&key)?
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(CONTEXT),
                &mut ciphertext,
            )
            .map_err(|_| anyhow!("pairing code verification failed"))?;
        Ok(data.to_vec())
    }
}

/// Caller must enforce a desktop-owned expiry and a global per-offer attempt limit.
/// The public reply is deliberately identical for right and wrong passwords.
pub fn respond(code: &str, message: &[u8], offer: &[u8]) -> Result<Reply> {
    validate(code)?;
    ensure!(
        message.len() == MESSAGE_BYTES && offer.len() <= MAX_OFFER_BYTES,
        "invalid pairing input"
    );
    let (state, outbound) = Spake2::<Ed25519Group>::start_b(
        &Password::new(code.as_bytes()),
        &Identity::new(PHONE),
        &Identity::new(DESKTOP),
    );
    let key = state
        .finish(message)
        .map_err(|_| anyhow!("invalid pairing message"))?;
    let mut nonce = [0u8; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| anyhow!("secure random failed"))?;
    let mut ciphertext = offer.to_vec();
    cipher(&key)?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(CONTEXT),
            &mut ciphertext,
        )
        .map_err(|_| anyhow!("pairing encryption failed"))?;
    let mut sealed = nonce.to_vec();
    sealed.extend(ciphertext);
    Ok(Reply {
        message: outbound,
        sealed,
    })
}

pub fn new_code() -> Result<String> {
    // Rejection sampling: floor(2^32 / 1,000,000) * 1,000,000 = 4,294,000,000.
    // Values above that boundary are discarded, so every six-digit code is equally likely.
    loop {
        let mut bytes = [0u8; 4];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| anyhow!("secure random failed"))?;
        let value = u32::from_be_bytes(bytes);
        if value < 4_294_000_000 {
            return Ok(format!("{:06}", value % 1_000_000));
        }
    }
}

fn validate(code: &str) -> Result<()> {
    ensure!(
        code.len() == 6 && code.bytes().all(|c| c.is_ascii_digit()),
        "invalid pairing code"
    );
    Ok(())
}
fn cipher(key: &[u8]) -> Result<aead::LessSafeKey> {
    // SPAKE2's transcript-derived 32-byte key is used for exactly one AES-GCM offer.
    Ok(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, key)
            .map_err(|_| anyhow!("invalid pairing key"))?,
    ))
}

use super::ServerIdentity;
use anyhow::{anyhow, ensure, Result};
use ring::{aead, agreement, hkdf, rand::SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// A relay payload is <=256 KiB: 8 sequence bytes + plaintext + 16 AES-GCM tag bytes.
const MAX_PLAINTEXT: usize = 256 * 1024 - 8 - 16;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelayHello {
    pub version: u32,
    pub client_key: String,
    pub nonce: String,
}

#[derive(Serialize)]
pub struct RelayHandshake {
    pub version: u32,
    pub server_key: String,
    pub certificate: String,
    pub signature: String,
}

/// One authenticated direction pair. Not Clone: copying counters would reuse GCM nonces.
/// A caller must discard the session on protocol/authentication failure and never serialize it.
pub struct RelaySession {
    incoming: aead::LessSafeKey,
    outgoing: aead::LessSafeKey,
    received: u64,
    sent: u64,
    transcript_hash: [u8; 32],
}

impl ServerIdentity {
    /// Authenticate this ephemeral exchange with the installation identity already pinned by iOS.
    /// Device tokens are sent only inside the resulting cipher and still require normal Mac auth.
    pub fn accept_relay_session(
        &self,
        stream: &str,
        hello: RelayHello,
    ) -> Result<(RelaySession, RelayHandshake)> {
        ensure!(
            stream.len() == 36
                && stream.bytes().enumerate().all(|(index, byte)| {
                    if matches!(index, 8 | 13 | 18 | 23) {
                        byte == b'-'
                    } else {
                        byte.is_ascii_hexdigit()
                    }
                }),
            "invalid relay stream"
        );
        ensure!(hello.version == 1, "unsupported relay protocol");
        let public = hex32(&hello.client_key)?;
        hex32(&hello.nonce)?;
        let private =
            agreement::EphemeralPrivateKey::generate(&agreement::X25519, &SystemRandom::new())
                .map_err(|_| anyhow!("relay key generation failed"))?;
        let server_key = hex::encode(
            private
                .compute_public_key()
                .map_err(|_| anyhow!("relay public key failed"))?,
        );
        let transcript = format!(
            "Norma collaboration session v1\n{stream}\n{}\n{}\n{server_key}\n{}",
            hello.nonce,
            hello.client_key,
            self.machine_id()
        );
        let transcript_hash: [u8; 32] = Sha256::digest(transcript.as_bytes()).into();
        let remote = agreement::UnparsedPublicKey::new(&agreement::X25519, public);
        let session = agreement::agree_ephemeral(private, &remote, |secret| {
            let extracted = hkdf::Salt::new(hkdf::HKDF_SHA256, &transcript_hash).extract(secret);
            let mut keys = [0u8; 64];
            extracted
                .expand(&[b"Norma collaboration keys v1"], KeyLength)
                .map_err(|_| anyhow!("relay key derivation failed"))?
                .fill(&mut keys)
                .map_err(|_| anyhow!("relay key derivation failed"))?;
            let incoming = cipher(&keys[..32]);
            let outgoing = cipher(&keys[32..]);
            keys.fill(0);
            Ok::<_, anyhow::Error>(RelaySession {
                incoming: incoming?,
                outgoing: outgoing?,
                received: 0,
                sent: 0,
                transcript_hash,
            })
        })
        .map_err(|_| anyhow!("invalid relay peer key"))??;
        let handshake = RelayHandshake {
            version: 1,
            server_key,
            certificate: hex::encode(self.certificate_der()),
            signature: hex::encode(self.sign(transcript.as_bytes())?),
        };
        Ok((session, handshake))
    }
}

impl RelaySession {
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        ensure!(plaintext.len() <= MAX_PLAINTEXT, "relay message too large");
        let sequence = self.sent;
        self.sent = self
            .sent
            .checked_add(1)
            .ok_or_else(|| anyhow!("relay sequence exhausted"))?;
        let mut encrypted = plaintext.to_vec();
        self.outgoing
            .seal_in_place_append_tag(nonce(sequence), self.aad(1, sequence), &mut encrypted)
            .map_err(|_| anyhow!("relay encryption failed"))?;
        let mut frame = sequence.to_be_bytes().to_vec();
        frame.extend(encrypted);
        Ok(frame)
    }

    pub fn open(&mut self, frame: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            (24..=MAX_PLAINTEXT + 24).contains(&frame.len()),
            "invalid relay frame length"
        );
        let sequence = u64::from_be_bytes(frame[..8].try_into()?);
        ensure!(sequence == self.received, "invalid relay sequence");
        let next = self
            .received
            .checked_add(1)
            .ok_or_else(|| anyhow!("relay sequence exhausted"))?;
        let mut ciphertext = frame[8..].to_vec();
        let plaintext = self
            .incoming
            .open_in_place(nonce(sequence), self.aad(0, sequence), &mut ciphertext)
            .map_err(|_| anyhow!("relay authentication failed"))?;
        self.received = next;
        Ok(plaintext.to_vec())
    }

    fn aad(&self, direction: u8, sequence: u64) -> aead::Aad<Vec<u8>> {
        let mut value = self.transcript_hash.to_vec();
        value.push(direction);
        value.extend_from_slice(&sequence.to_be_bytes());
        aead::Aad::from(value)
    }
}

struct KeyLength;
impl hkdf::KeyType for KeyLength {
    fn len(&self) -> usize {
        64
    }
}

fn hex32(value: &str) -> Result<Vec<u8>> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid relay key or nonce"
    );
    Ok(hex::decode(value)?)
}

fn cipher(bytes: &[u8]) -> Result<aead::LessSafeKey> {
    Ok(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, bytes)
            .map_err(|_| anyhow!("invalid relay cipher"))?,
    ))
}

fn nonce(sequence: u64) -> aead::Nonce {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&sequence.to_be_bytes());
    aead::Nonce::assume_unique_for_key(nonce)
}

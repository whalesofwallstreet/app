//! Local cryptographic verification of Circle CCTP attestations.
//!
//! Instead of trusting the response of Circle's centralized attestation API,
//! this module independently verifies that an attestation was produced by the
//! authorized Circle attester set. Verification is pure, synchronous and
//! bounded: it recovers the secp256k1 signer of each 65-byte signature over
//! the keccak256 digest of the raw `MessageTransmitter` message and checks the
//! recovered addresses against a cached on-chain attester registry.
//!
//! The attester registry itself is refreshed asynchronously (and cached) from
//! the source-chain `MessageTransmitter` contract through an
//! [`AttesterKeySource`], so key rotations performed by Circle on-chain are
//! picked up automatically without redeploying the engine.

use crate::bridge::Chain;
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use sha3::{Digest, Keccak256};
use std::collections::{BTreeSet, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use thiserror::Error;

/// A 20-byte Ethereum-style address derived from a secp256k1 public key.
pub type AttesterAddress = [u8; 20];

/// Byte length of one recoverable ECDSA signature (r || s || v).
const SIGNATURE_LEN: usize = 65;

/// Upper bound on signatures accepted in a single attestation. Keeps the
/// synchronous verification path bounded regardless of input size.
const MAX_SIGNATURES: usize = 16;

/// Fixed header layout of a CCTP `MessageTransmitter` message:
/// version (4) + source domain (4) + destination domain (4) + nonce (8) +
/// sender (32) + recipient (32) + destination caller (32).
const MESSAGE_HEADER_LEN: usize = 116;

/// The only message format version this verifier understands.
const SUPPORTED_MESSAGE_VERSION: u32 = 0;

/// How long a fetched attester set is trusted before it is re-fetched from
/// the source chain.
const DEFAULT_KEY_CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Error, Debug, PartialEq, Eq)]
pub enum AttestationError {
    #[error("malformed message: {0}")]
    MalformedMessage(String),

    #[error(
        "malformed attestation: length {0} is not a non-empty multiple of 65 bytes (max {max})",
        max = MAX_SIGNATURES * SIGNATURE_LEN
    )]
    MalformedAttestation(usize),

    #[error("unsupported message version {0}, expected {SUPPORTED_MESSAGE_VERSION}")]
    UnsupportedVersion(u32),

    #[error(
        "destination domain mismatch: message targets {actual}, but the requested destination chain is domain {expected}"
    )]
    DomainMismatch { expected: u32, actual: u32 },

    #[error("nonce {nonce} from source domain {source_domain} was already consumed (replay)")]
    ReplayedNonce { source_domain: u32, nonce: u64 },

    #[error("signature {index} failed cryptographic verification")]
    InvalidSignature { index: usize },

    #[error("signature {index} uses a malleable (high-s) encoding")]
    MalleableSignature { index: usize },

    #[error(
        "signature {index} was produced by 0x{}, which is not an enabled attester",
        hex::encode(signer)
    )]
    UnknownAttester {
        index: usize,
        signer: AttesterAddress,
    },

    #[error("signatures are not in strictly increasing signer-address order at index {index}")]
    NonIncreasingSigners { index: usize },

    #[error("only {got} valid attester signatures, threshold is {required}")]
    ThresholdNotMet { got: usize, required: usize },

    #[error("attester key source unavailable and no cached key set exists")]
    KeySourceUnavailable,

    #[error("nonce store unavailable, failing closed: {0}")]
    NonceStoreUnavailable(String),
}

/// CCTP domain identifier for each chain the engine routes to, following
/// Circle's domain numbering. The destination domain expected during
/// verification is derived from the actual destination chain of the request,
/// never from static configuration.
pub fn cctp_domain(chain: Chain) -> u32 {
    match chain {
        Chain::Ethereum => 0,
        Chain::Arbitrum => 3,
        Chain::Solana => 5,
        // Stellar is not a native CCTP chain. It gets an engine-assigned
        // identifier far outside Circle's allocated range so a forged
        // message can never alias a real CCTP domain.
        Chain::Stellar => 1_000_000,
    }
}

/// Decoded header of a raw CCTP `MessageTransmitter` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CctpMessage {
    pub version: u32,
    pub source_domain: u32,
    pub destination_domain: u32,
    pub nonce: u64,
    pub sender: [u8; 32],
    pub recipient: [u8; 32],
    pub destination_caller: [u8; 32],
    pub message_body: Vec<u8>,
}

impl CctpMessage {
    /// Parses the fixed-layout header of a raw CCTP message.
    pub fn parse(raw: &[u8]) -> Result<Self, AttestationError> {
        if raw.len() < MESSAGE_HEADER_LEN {
            return Err(AttestationError::MalformedMessage(format!(
                "message is {} bytes, header requires at least {}",
                raw.len(),
                MESSAGE_HEADER_LEN
            )));
        }

        let mut fixed = [0u8; 4];
        fixed.copy_from_slice(&raw[0..4]);
        let version = u32::from_be_bytes(fixed);
        fixed.copy_from_slice(&raw[4..8]);
        let source_domain = u32::from_be_bytes(fixed);
        fixed.copy_from_slice(&raw[8..12]);
        let destination_domain = u32::from_be_bytes(fixed);

        let mut nonce_bytes = [0u8; 8];
        nonce_bytes.copy_from_slice(&raw[12..20]);
        let nonce = u64::from_be_bytes(nonce_bytes);

        let mut sender = [0u8; 32];
        sender.copy_from_slice(&raw[20..52]);
        let mut recipient = [0u8; 32];
        recipient.copy_from_slice(&raw[52..84]);
        let mut destination_caller = [0u8; 32];
        destination_caller.copy_from_slice(&raw[84..116]);

        Ok(Self {
            version,
            source_domain,
            destination_domain,
            nonce,
            sender,
            recipient,
            destination_caller,
            message_body: raw[MESSAGE_HEADER_LEN..].to_vec(),
        })
    }
}

/// The set of attester keys currently authorized on the source chain,
/// together with the on-chain `signatureThreshold`.
#[derive(Debug, Clone)]
pub struct AttesterSet {
    attesters: BTreeSet<AttesterAddress>,
    threshold: usize,
}

impl AttesterSet {
    pub fn new(
        attesters: impl IntoIterator<Item = AttesterAddress>,
        threshold: usize,
    ) -> Result<Self, anyhow::Error> {
        let attesters: BTreeSet<AttesterAddress> = attesters.into_iter().collect();
        if threshold == 0 {
            anyhow::bail!("signature threshold must be at least 1");
        }
        if attesters.len() < threshold {
            anyhow::bail!(
                "attester set of size {} cannot satisfy threshold {}",
                attesters.len(),
                threshold
            );
        }
        Ok(Self {
            attesters,
            threshold,
        })
    }

    pub fn contains(&self, address: &AttesterAddress) -> bool {
        self.attesters.contains(address)
    }

    pub fn threshold(&self) -> usize {
        self.threshold
    }
}

/// Source of truth for the enabled attester keys. Production implementations
/// fetch `getNumEnabledAttesters` / `getEnabledAttester` / `signatureThreshold`
/// from the `MessageTransmitter` contract over JSON-RPC; tests plug in a
/// static set.
pub trait AttesterKeySource: Send + Sync {
    fn fetch(&self) -> impl Future<Output = Result<AttesterSet, anyhow::Error>> + Send;
}

/// A fixed attester set, used as the bootstrap registry and in tests.
#[derive(Debug, Clone)]
pub struct StaticKeySource {
    set: AttesterSet,
}

impl StaticKeySource {
    pub fn new(set: AttesterSet) -> Self {
        Self { set }
    }
}

impl AttesterKeySource for StaticKeySource {
    fn fetch(&self) -> impl Future<Output = Result<AttesterSet, anyhow::Error>> + Send {
        let set = self.set.clone();
        async move { Ok(set) }
    }
}

/// Fetches the enabled attester set and signature threshold from the
/// `MessageTransmitter` contract over JSON-RPC (`eth_call`), so on-chain key
/// rotations propagate to the verifier without a redeploy.
pub struct RpcKeySource {
    client: reqwest_middleware::ClientWithMiddleware,
    rpc_url: String,
    message_transmitter: String,
}

impl RpcKeySource {
    pub fn new(
        client: reqwest_middleware::ClientWithMiddleware,
        rpc_url: impl Into<String>,
        message_transmitter: impl Into<String>,
    ) -> Self {
        Self {
            client,
            rpc_url: rpc_url.into(),
            message_transmitter: message_transmitter.into(),
        }
    }

    async fn eth_call(&self, calldata: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [
                {
                    "to": self.message_transmitter,
                    "data": format!("0x{}", hex::encode(calldata)),
                },
                "latest"
            ],
        });

        let response: serde_json::Value = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if let Some(err) = response.get("error") {
            anyhow::bail!("eth_call failed: {err}");
        }
        let result = response
            .get("result")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("eth_call response missing result"))?;
        Ok(hex::decode(result.trim_start_matches("0x"))?)
    }

    async fn call_u64(&self, signature: &str) -> Result<u64, anyhow::Error> {
        let word = self.eth_call(&selector(signature)).await?;
        decode_u64_word(&word)
    }
}

impl AttesterKeySource for RpcKeySource {
    async fn fetch(&self) -> Result<AttesterSet, anyhow::Error> {
        let threshold = self.call_u64("signatureThreshold()").await?;
        let count = self.call_u64("getNumEnabledAttesters()").await?;
        if count as usize > MAX_SIGNATURES {
            anyhow::bail!(
                "contract reports {count} attesters, above the bound of {MAX_SIGNATURES}"
            );
        }

        let mut attesters = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut calldata = selector("getEnabledAttester(uint256)").to_vec();
            let mut word = [0u8; 32];
            word[24..].copy_from_slice(&index.to_be_bytes());
            calldata.extend_from_slice(&word);

            let result = self.eth_call(&calldata).await?;
            attesters.push(decode_address_word(&result)?);
        }

        AttesterSet::new(attesters, threshold as usize)
    }
}

/// First four bytes of keccak256 over the Solidity function signature.
fn selector(signature: &str) -> [u8; 4] {
    let hash = Keccak256::digest(signature.as_bytes());
    [hash[0], hash[1], hash[2], hash[3]]
}

/// Decodes a `uint256` ABI word into a `u64`, rejecting values that overflow.
fn decode_u64_word(word: &[u8]) -> Result<u64, anyhow::Error> {
    if word.len() != 32 || word[..24].iter().any(|b| *b != 0) {
        anyhow::bail!("eth_call returned a malformed uint256 word");
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&word[24..]);
    Ok(u64::from_be_bytes(bytes))
}

/// Decodes an `address` ABI word (12 zero bytes of padding + 20 bytes).
fn decode_address_word(word: &[u8]) -> Result<AttesterAddress, anyhow::Error> {
    if word.len() != 32 || word[..12].iter().any(|b| *b != 0) {
        anyhow::bail!("eth_call returned a malformed address word");
    }
    let mut address = [0u8; 20];
    address.copy_from_slice(&word[12..]);
    Ok(address)
}

struct CachedAttesters {
    set: AttesterSet,
    fetched_at: Instant,
}

/// Records consumed (source domain, nonce) pairs so a burn message can only
/// be minted once. Implementations must survive process restarts: an
/// attestation consumed before a redeploy must still be rejected after it.
pub trait NonceStore: Send + Sync {
    fn is_consumed(&self, source_domain: u32, nonce: u64) -> Result<bool, anyhow::Error>;
    fn mark_consumed(&self, source_domain: u32, nonce: u64) -> Result<(), anyhow::Error>;
}

/// Volatile store for tests and ephemeral tooling. Production deployments
/// should use [`FileNonceStore`] (or another durable backend) so replay
/// protection is not wiped by a restart.
#[derive(Default)]
pub struct InMemoryNonceStore {
    consumed: Mutex<HashSet<(u32, u64)>>,
}

impl NonceStore for InMemoryNonceStore {
    fn is_consumed(&self, source_domain: u32, nonce: u64) -> Result<bool, anyhow::Error> {
        Ok(self
            .consumed
            .lock()
            .expect("nonce set lock poisoned")
            .contains(&(source_domain, nonce)))
    }

    fn mark_consumed(&self, source_domain: u32, nonce: u64) -> Result<(), anyhow::Error> {
        self.consumed
            .lock()
            .expect("nonce set lock poisoned")
            .insert((source_domain, nonce));
        Ok(())
    }
}

/// Durable store backed by an append-only log file. The full set is loaded
/// into memory at startup; every newly consumed nonce is appended and
/// flushed before verification succeeds, so a consumed attestation stays
/// consumed across restarts and redeploys.
pub struct FileNonceStore {
    state: Mutex<FileNonceState>,
}

struct FileNonceState {
    consumed: HashSet<(u32, u64)>,
    writer: std::io::BufWriter<std::fs::File>,
}

impl FileNonceStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, anyhow::Error> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut consumed = HashSet::new();
        if path.exists() {
            for line in std::fs::read_to_string(&path)?.lines() {
                let mut parts = line.split_whitespace();
                if let (Some(domain), Some(nonce)) = (parts.next(), parts.next()) {
                    consumed.insert((domain.parse()?, nonce.parse()?));
                }
            }
        }

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        Ok(Self {
            state: Mutex::new(FileNonceState {
                consumed,
                writer: std::io::BufWriter::new(file),
            }),
        })
    }
}

impl NonceStore for FileNonceStore {
    fn is_consumed(&self, source_domain: u32, nonce: u64) -> Result<bool, anyhow::Error> {
        Ok(self
            .state
            .lock()
            .expect("nonce store lock poisoned")
            .consumed
            .contains(&(source_domain, nonce)))
    }

    fn mark_consumed(&self, source_domain: u32, nonce: u64) -> Result<(), anyhow::Error> {
        use std::io::Write;
        let mut state = self.state.lock().expect("nonce store lock poisoned");
        if state.consumed.insert((source_domain, nonce)) {
            writeln!(state.writer, "{source_domain} {nonce}")?;
            state.writer.flush()?;
        }
        Ok(())
    }
}

/// Verifies CCTP attestations against a cached, auto-rotating attester set.
pub struct AttestationVerifier<S: AttesterKeySource> {
    source: S,
    cache_ttl: Duration,
    cache: Mutex<Option<CachedAttesters>>,
    nonce_store: Box<dyn NonceStore>,
}

impl<S: AttesterKeySource> AttestationVerifier<S> {
    pub fn new(source: S, nonce_store: Box<dyn NonceStore>) -> Self {
        Self {
            source,
            cache_ttl: DEFAULT_KEY_CACHE_TTL,
            cache: Mutex::new(None),
            nonce_store,
        }
    }

    #[cfg(test)]
    fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Verifies an attestation over a raw CCTP message.
    ///
    /// `expected_destination_domain` must be derived from the actual
    /// destination chain of the request (see [`cctp_domain`]); messages
    /// addressed to any other domain are rejected so an attestation for one
    /// chain can never be replayed against another.
    ///
    /// Structural checks (bounds, parse, domain, replay) run synchronously
    /// before any network access; the attester set is only fetched (or read
    /// from cache) once the message has passed them. The cryptographic core
    /// is bounded and synchronous.
    #[tracing::instrument(skip_all, err)]
    pub async fn verify(
        &self,
        message: &[u8],
        attestation: &[u8],
        expected_destination_domain: u32,
    ) -> Result<CctpMessage, AttestationError> {
        let parsed = self.precheck(message, attestation, expected_destination_domain)?;
        let attesters = self.current_attesters().await?;
        self.verify_signatures(message, attestation, &attesters)?;

        // Persist before reporting success: if the store fails we fail
        // closed rather than allow an unrecorded (replayable) mint.
        self.nonce_store
            .mark_consumed(parsed.source_domain, parsed.nonce)
            .map_err(|err| AttestationError::NonceStoreUnavailable(err.to_string()))?;

        Ok(parsed)
    }

    /// Returns the cached attester set, re-fetching it from the key source
    /// when stale. A stale cache is served as a fallback if the source is
    /// temporarily unreachable, so verification degrades gracefully during
    /// RPC outages instead of hard-failing.
    async fn current_attesters(&self) -> Result<AttesterSet, AttestationError> {
        {
            let cache = self.cache.lock().expect("attester cache lock poisoned");
            if let Some(cached) = cache.as_ref() {
                if cached.fetched_at.elapsed() < self.cache_ttl {
                    return Ok(cached.set.clone());
                }
            }
        }

        match self.source.fetch().await {
            Ok(set) => {
                let mut cache = self.cache.lock().expect("attester cache lock poisoned");
                *cache = Some(CachedAttesters {
                    set: set.clone(),
                    fetched_at: Instant::now(),
                });
                Ok(set)
            }
            Err(err) => {
                tracing::warn!("attester key refresh failed, using cached set: {err:?}");
                let cache = self.cache.lock().expect("attester cache lock poisoned");
                cache
                    .as_ref()
                    .map(|cached| cached.set.clone())
                    .ok_or(AttestationError::KeySourceUnavailable)
            }
        }
    }

    /// Synchronous structural validation: bounds, header parse, version,
    /// destination domain and nonce replay. No cryptography, no network.
    fn precheck(
        &self,
        message: &[u8],
        attestation: &[u8],
        expected_destination_domain: u32,
    ) -> Result<CctpMessage, AttestationError> {
        if attestation.is_empty()
            || !attestation.len().is_multiple_of(SIGNATURE_LEN)
            || attestation.len() > MAX_SIGNATURES * SIGNATURE_LEN
        {
            return Err(AttestationError::MalformedAttestation(attestation.len()));
        }

        let parsed = CctpMessage::parse(message)?;
        if parsed.version != SUPPORTED_MESSAGE_VERSION {
            return Err(AttestationError::UnsupportedVersion(parsed.version));
        }
        if parsed.destination_domain != expected_destination_domain {
            return Err(AttestationError::DomainMismatch {
                expected: expected_destination_domain,
                actual: parsed.destination_domain,
            });
        }

        let consumed = self
            .nonce_store
            .is_consumed(parsed.source_domain, parsed.nonce)
            .map_err(|err| AttestationError::NonceStoreUnavailable(err.to_string()))?;
        if consumed {
            return Err(AttestationError::ReplayedNonce {
                source_domain: parsed.source_domain,
                nonce: parsed.nonce,
            });
        }

        Ok(parsed)
    }

    /// Pure, synchronous, bounded signature verification of `attestation`
    /// over `message` against a concrete attester set.
    fn verify_signatures(
        &self,
        message: &[u8],
        attestation: &[u8],
        attesters: &AttesterSet,
    ) -> Result<(), AttestationError> {
        let digest: [u8; 32] = Keccak256::digest(message).into();

        // Mirrors MessageTransmitter's on-chain rule: signatures must be
        // ordered by strictly increasing signer address, which also makes
        // duplicate signers impossible.
        let mut previous_signer: Option<AttesterAddress> = None;
        let mut valid_signatures = 0usize;

        for (index, raw_sig) in attestation.chunks_exact(SIGNATURE_LEN).enumerate() {
            let signer = recover_signer(&digest, raw_sig, index)?;

            if !attesters.contains(&signer) {
                return Err(AttestationError::UnknownAttester { index, signer });
            }
            if let Some(prev) = previous_signer {
                if signer <= prev {
                    return Err(AttestationError::NonIncreasingSigners { index });
                }
            }
            previous_signer = Some(signer);
            valid_signatures += 1;
        }

        if valid_signatures < attesters.threshold() {
            return Err(AttestationError::ThresholdNotMet {
                got: valid_signatures,
                required: attesters.threshold(),
            });
        }

        Ok(())
    }
}

/// Recovers the Ethereum-style address that produced a 65-byte
/// (r || s || v) recoverable signature over `digest`.
fn recover_signer(
    digest: &[u8; 32],
    raw_sig: &[u8],
    index: usize,
) -> Result<AttesterAddress, AttestationError> {
    let signature = Signature::from_slice(&raw_sig[..64])
        .map_err(|_| AttestationError::InvalidSignature { index })?;

    // Reject the high-s twin of every signature so each message admits
    // exactly one canonical encoding per signer.
    if signature.normalize_s().is_some() {
        return Err(AttestationError::MalleableSignature { index });
    }

    let v = raw_sig[64];
    let recovery_id = v
        .checked_sub(27)
        .and_then(RecoveryId::from_byte)
        .ok_or(AttestationError::InvalidSignature { index })?;

    let verifying_key = VerifyingKey::recover_from_prehash(digest, &signature, recovery_id)
        .map_err(|_| AttestationError::InvalidSignature { index })?;

    Ok(address_from_key(&verifying_key))
}

/// Last 20 bytes of keccak256 over the uncompressed public key (minus the
/// 0x04 prefix), i.e. the standard Ethereum address derivation.
pub fn address_from_key(key: &VerifyingKey) -> AttesterAddress {
    let uncompressed = key.to_encoded_point(false);
    let hash = Keccak256::digest(&uncompressed.as_bytes()[1..]);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    address
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::SigningKey;

    /// Deterministic test attester keys. The corresponding addresses are
    /// asserted below as hardcoded vectors so any drift in the address
    /// derivation is caught immediately.
    const ATTESTER_1_SECRET: [u8; 32] = [
        0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
        0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
        0x11, 0x11,
    ];
    const ATTESTER_2_SECRET: [u8; 32] = [
        0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22,
        0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22,
        0x22, 0x22,
    ];
    const ROGUE_SECRET: [u8; 32] = [
        0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99,
        0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99,
        0x99, 0x99,
    ];

    const LOCAL_DOMAIN: u32 = 3; // Arbitrum in CCTP domain numbering

    fn signing_key(secret: &[u8; 32]) -> SigningKey {
        SigningKey::from_bytes(secret.into()).expect("valid test scalar")
    }

    fn sign(message: &[u8], secret: &[u8; 32]) -> Vec<u8> {
        let digest: [u8; 32] = Keccak256::digest(message).into();
        let (signature, recovery_id) = signing_key(secret)
            .sign_prehash_recoverable(&digest)
            .expect("signing cannot fail");
        let mut out = signature.to_vec();
        out.push(recovery_id.to_byte() + 27);
        out
    }

    /// Concatenates signatures ordered by ascending signer address, as the
    /// MessageTransmitter contract requires.
    fn attest(message: &[u8], secrets: &[&[u8; 32]]) -> Vec<u8> {
        let mut sigs: Vec<(AttesterAddress, Vec<u8>)> = secrets
            .iter()
            .map(|secret| {
                let address = address_from_key(signing_key(secret).verifying_key());
                (address, sign(message, secret))
            })
            .collect();
        sigs.sort_by_key(|sig| sig.0);
        sigs.into_iter().flat_map(|(_, sig)| sig).collect()
    }

    fn test_message(source_domain: u32, destination_domain: u32, nonce: u64) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(&0u32.to_be_bytes()); // version
        message.extend_from_slice(&source_domain.to_be_bytes());
        message.extend_from_slice(&destination_domain.to_be_bytes());
        message.extend_from_slice(&nonce.to_be_bytes());
        message.extend_from_slice(&[0xAA; 32]); // sender
        message.extend_from_slice(&[0xBB; 32]); // recipient
        message.extend_from_slice(&[0x00; 32]); // destination caller
        message.extend_from_slice(b"depositForBurn:1000000:USDC");
        message
    }

    fn verifier(threshold: usize) -> AttestationVerifier<StaticKeySource> {
        let set = AttesterSet::new(
            [
                address_from_key(signing_key(&ATTESTER_1_SECRET).verifying_key()),
                address_from_key(signing_key(&ATTESTER_2_SECRET).verifying_key()),
            ],
            threshold,
        )
        .unwrap();
        AttestationVerifier::new(
            StaticKeySource::new(set),
            Box::new(InMemoryNonceStore::default()),
        )
    }

    #[test]
    fn test_address_derivation_matches_hardcoded_vectors() {
        // Independently computed with standard Ethereum tooling from the
        // fixed secret scalars above.
        assert_eq!(
            hex::encode(address_from_key(
                signing_key(&ATTESTER_1_SECRET).verifying_key()
            )),
            "19e7e376e7c213b7e7e7e46cc70a5dd086daff2a"
        );
        assert_eq!(
            hex::encode(address_from_key(
                signing_key(&ATTESTER_2_SECRET).verifying_key()
            )),
            "1563915e194d8cfba1943570603f7606a3115508"
        );
    }

    #[tokio::test]
    async fn test_valid_attestation_verifies_locally() {
        let verifier = verifier(2);
        let message = test_message(0, LOCAL_DOMAIN, 42);
        let attestation = attest(&message, &[&ATTESTER_1_SECRET, &ATTESTER_2_SECRET]);

        let parsed = verifier
            .verify(&message, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap();
        assert_eq!(parsed.source_domain, 0);
        assert_eq!(parsed.nonce, 42);
        assert_eq!(parsed.message_body, b"depositForBurn:1000000:USDC");
    }

    #[tokio::test]
    async fn test_tampered_payload_is_rejected() {
        let verifier = verifier(1);
        let message = test_message(0, LOCAL_DOMAIN, 1);
        let attestation = attest(&message, &[&ATTESTER_1_SECRET]);

        // Flip one byte of the message body after signing.
        let mut tampered = message.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;

        let err = verifier
            .verify(&tampered, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap_err();
        // Recovery over a different digest yields a different (unknown)
        // signer, or an outright invalid signature.
        assert!(matches!(
            err,
            AttestationError::UnknownAttester { .. } | AttestationError::InvalidSignature { .. }
        ));
    }

    #[tokio::test]
    async fn test_unknown_attester_is_rejected() {
        let verifier = verifier(1);
        let message = test_message(0, LOCAL_DOMAIN, 2);
        let attestation = attest(&message, &[&ROGUE_SECRET]);

        let err = verifier
            .verify(&message, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap_err();
        assert!(matches!(err, AttestationError::UnknownAttester { .. }));
    }

    #[tokio::test]
    async fn test_threshold_not_met_is_rejected() {
        let verifier = verifier(2);
        let message = test_message(0, LOCAL_DOMAIN, 3);
        let attestation = attest(&message, &[&ATTESTER_1_SECRET]);

        let err = verifier
            .verify(&message, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            AttestationError::ThresholdNotMet {
                got: 1,
                required: 2
            }
        );
    }

    #[tokio::test]
    async fn test_replayed_nonce_is_rejected() {
        let verifier = verifier(1);
        let message = test_message(0, LOCAL_DOMAIN, 7);
        let attestation = attest(&message, &[&ATTESTER_1_SECRET]);

        verifier
            .verify(&message, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap();
        let err = verifier
            .verify(&message, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            AttestationError::ReplayedNonce {
                source_domain: 0,
                nonce: 7
            }
        );

        // A different nonce from the same source domain still verifies.
        let next = test_message(0, LOCAL_DOMAIN, 8);
        let next_attestation = attest(&next, &[&ATTESTER_1_SECRET]);
        verifier
            .verify(&next, &next_attestation, LOCAL_DOMAIN)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_wrong_destination_domain_is_rejected() {
        let verifier = verifier(1);
        let message = test_message(0, LOCAL_DOMAIN + 1, 9);
        let attestation = attest(&message, &[&ATTESTER_1_SECRET]);

        let err = verifier
            .verify(&message, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            AttestationError::DomainMismatch {
                expected: LOCAL_DOMAIN,
                actual: LOCAL_DOMAIN + 1
            }
        );
    }

    #[tokio::test]
    async fn test_unordered_signatures_are_rejected() {
        let verifier = verifier(2);
        let message = test_message(0, LOCAL_DOMAIN, 10);

        // Build the attestation in descending signer-address order.
        let ordered = attest(&message, &[&ATTESTER_1_SECRET, &ATTESTER_2_SECRET]);
        let mut reversed = ordered[SIGNATURE_LEN..].to_vec();
        reversed.extend_from_slice(&ordered[..SIGNATURE_LEN]);

        let err = verifier
            .verify(&message, &reversed, LOCAL_DOMAIN)
            .await
            .unwrap_err();
        assert_eq!(err, AttestationError::NonIncreasingSigners { index: 1 });
    }

    #[tokio::test]
    async fn test_malformed_attestation_lengths_are_rejected() {
        let verifier = verifier(1);
        let message = test_message(0, LOCAL_DOMAIN, 11);

        for bad in [
            Vec::new(),
            vec![0u8; SIGNATURE_LEN - 1],
            vec![0u8; SIGNATURE_LEN + 1],
            vec![0u8; (MAX_SIGNATURES + 1) * SIGNATURE_LEN],
        ] {
            let err = verifier
                .verify(&message, &bad, LOCAL_DOMAIN)
                .await
                .unwrap_err();
            assert!(matches!(err, AttestationError::MalformedAttestation(_)));
        }
    }

    #[tokio::test]
    async fn test_garbage_signature_is_rejected() {
        let verifier = verifier(1);
        let message = test_message(0, LOCAL_DOMAIN, 12);

        let err = verifier
            .verify(&message, &[0x01u8; SIGNATURE_LEN], LOCAL_DOMAIN)
            .await
            .unwrap_err();
        assert!(matches!(err, AttestationError::InvalidSignature { .. }));
    }

    #[tokio::test]
    async fn test_high_s_malleable_signature_is_rejected() {
        let verifier = verifier(1);
        let message = test_message(0, LOCAL_DOMAIN, 13);
        let mut attestation = attest(&message, &[&ATTESTER_1_SECRET]);

        // Re-encode the signature as its high-s twin (s' = n - s, flipped
        // recovery id): same message, same signer, different bytes.
        let signature = Signature::from_slice(&attestation[..64]).unwrap();
        let high_s = Signature::from_scalars(*signature.r(), -*signature.s()).unwrap();
        attestation[..64].copy_from_slice(&high_s.to_bytes());
        attestation[64] = if attestation[64] == 27 { 28 } else { 27 };

        let err = verifier
            .verify(&message, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap_err();
        assert_eq!(err, AttestationError::MalleableSignature { index: 0 });
    }

    #[tokio::test]
    async fn test_truncated_message_is_rejected() {
        let verifier = verifier(1);
        let err = verifier
            .verify(
                &[0u8; MESSAGE_HEADER_LEN - 1],
                &[0u8; SIGNATURE_LEN],
                LOCAL_DOMAIN,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AttestationError::MalformedMessage(_)));
    }

    #[tokio::test]
    async fn test_rpc_key_source_decodes_contract_state() {
        use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

        let attester_1 = address_from_key(signing_key(&ATTESTER_1_SECRET).verifying_key());
        let attester_2 = address_from_key(signing_key(&ATTESTER_2_SECRET).verifying_key());

        // Dispatches eth_call requests on the ABI selector in the calldata,
        // emulating the MessageTransmitter contract's read methods.
        struct FakeTransmitter {
            attesters: Vec<AttesterAddress>,
        }
        impl Respond for FakeTransmitter {
            fn respond(&self, request: &Request) -> ResponseTemplate {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let data = body["params"][0]["data"].as_str().unwrap();
                let calldata = hex::decode(data.trim_start_matches("0x")).unwrap();

                let mut word = [0u8; 32];
                if calldata[..4] == selector("signatureThreshold()") {
                    word[31] = 2;
                } else if calldata[..4] == selector("getNumEnabledAttesters()") {
                    word[31] = self.attesters.len() as u8;
                } else if calldata[..4] == selector("getEnabledAttester(uint256)") {
                    let index = calldata[35] as usize;
                    word[12..].copy_from_slice(&self.attesters[index]);
                } else {
                    return ResponseTemplate::new(400);
                }

                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": format!("0x{}", hex::encode(word)),
                }))
            }
        }

        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::any())
            .respond_with(FakeTransmitter {
                attesters: vec![attester_1, attester_2],
            })
            .mount(&server)
            .await;

        let source = RpcKeySource::new(
            crate::http_client::build_resilient_client().unwrap(),
            server.uri(),
            "0x0a992d191deec32afe36203ad87d7d289a738f81",
        );
        let set = source.fetch().await.unwrap();

        assert_eq!(set.threshold(), 2);
        assert!(set.contains(&attester_1));
        assert!(set.contains(&attester_2));

        // The fetched set plugs straight into end-to-end verification.
        let verifier = AttestationVerifier::new(
            StaticKeySource::new(set),
            Box::new(InMemoryNonceStore::default()),
        );
        let message = test_message(0, LOCAL_DOMAIN, 30);
        let attestation = attest(&message, &[&ATTESTER_1_SECRET, &ATTESTER_2_SECRET]);
        verifier
            .verify(&message, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap();
    }

    #[test]
    fn test_cctp_domain_mapping() {
        // Circle's published domain numbering for natively supported chains.
        assert_eq!(cctp_domain(Chain::Ethereum), 0);
        assert_eq!(cctp_domain(Chain::Arbitrum), 3);
        assert_eq!(cctp_domain(Chain::Solana), 5);
        // Stellar's engine-assigned identifier must not collide with any
        // other routed chain.
        let domains: std::collections::HashSet<u32> = [
            Chain::Ethereum,
            Chain::Arbitrum,
            Chain::Solana,
            Chain::Stellar,
        ]
        .into_iter()
        .map(cctp_domain)
        .collect();
        assert_eq!(domains.len(), 4);
    }

    #[tokio::test]
    async fn test_destination_domain_is_per_request() {
        // One verifier serves every destination chain: the expected domain
        // comes from the request, not from static configuration.
        let verifier = verifier(1);

        for (dest_domain, nonce) in [(0u32, 40u64), (3, 41), (5, 42)] {
            let message = test_message(7, dest_domain, nonce);
            let attestation = attest(&message, &[&ATTESTER_1_SECRET]);

            // Accepted when the expected domain matches the request's
            // destination chain.
            verifier
                .verify(&message, &attestation, dest_domain)
                .await
                .unwrap();

            // The same attestation replayed against a different destination
            // domain is rejected before any cryptography runs.
            let other = test_message(7, dest_domain, nonce + 100);
            let other_attestation = attest(&other, &[&ATTESTER_1_SECRET]);
            let err = verifier
                .verify(&other, &other_attestation, dest_domain + 1)
                .await
                .unwrap_err();
            assert_eq!(
                err,
                AttestationError::DomainMismatch {
                    expected: dest_domain + 1,
                    actual: dest_domain
                }
            );
        }
    }

    #[tokio::test]
    async fn test_replay_is_rejected_across_restarts() {
        let dir = std::env::temp_dir().join(format!("wow-nonce-{}", uuid::Uuid::new_v4()));
        let path = dir.join("consumed_nonces.log");

        let set = AttesterSet::new(
            [address_from_key(
                signing_key(&ATTESTER_1_SECRET).verifying_key(),
            )],
            1,
        )
        .unwrap();

        let message = test_message(0, LOCAL_DOMAIN, 55);
        let attestation = attest(&message, &[&ATTESTER_1_SECRET]);

        // First process: consume the attestation.
        {
            let verifier = AttestationVerifier::new(
                StaticKeySource::new(set.clone()),
                Box::new(FileNonceStore::open(&path).unwrap()),
            );
            verifier
                .verify(&message, &attestation, LOCAL_DOMAIN)
                .await
                .unwrap();
        }

        // Second process (fresh store from the same file): the replay must
        // still be rejected even though all in-memory state is gone.
        let verifier = AttestationVerifier::new(
            StaticKeySource::new(set),
            Box::new(FileNonceStore::open(&path).unwrap()),
        );
        let err = verifier
            .verify(&message, &attestation, LOCAL_DOMAIN)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            AttestationError::ReplayedNonce {
                source_domain: 0,
                nonce: 55
            }
        );

        // A fresh nonce still verifies after the restart.
        let next = test_message(0, LOCAL_DOMAIN, 56);
        let next_attestation = attest(&next, &[&ATTESTER_1_SECRET]);
        verifier
            .verify(&next, &next_attestation, LOCAL_DOMAIN)
            .await
            .unwrap();

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_attester_rotation_is_picked_up_after_cache_expiry() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // Key source that serves attester 1 on the first fetch and rotates
        // to attester 2 afterwards, emulating an on-chain key rotation.
        struct RotatingSource {
            fetches: Arc<AtomicUsize>,
        }
        impl AttesterKeySource for RotatingSource {
            fn fetch(&self) -> impl Future<Output = Result<AttesterSet, anyhow::Error>> + Send {
                let n = self.fetches.fetch_add(1, Ordering::SeqCst);
                async move {
                    let secret = if n == 0 {
                        &ATTESTER_1_SECRET
                    } else {
                        &ATTESTER_2_SECRET
                    };
                    AttesterSet::new([address_from_key(signing_key(secret).verifying_key())], 1)
                }
            }
        }

        let fetches = Arc::new(AtomicUsize::new(0));
        let verifier = AttestationVerifier::new(
            RotatingSource {
                fetches: fetches.clone(),
            },
            Box::new(InMemoryNonceStore::default()),
        )
        .with_cache_ttl(Duration::ZERO);

        // Before rotation only attester 1 is accepted.
        let message = test_message(0, LOCAL_DOMAIN, 20);
        verifier
            .verify(
                &message,
                &attest(&message, &[&ATTESTER_1_SECRET]),
                LOCAL_DOMAIN,
            )
            .await
            .unwrap();

        // After the cache expires the rotated set applies: attester 1 is now
        // rejected and attester 2 is accepted.
        let message = test_message(0, LOCAL_DOMAIN, 21);
        let err = verifier
            .verify(
                &message,
                &attest(&message, &[&ATTESTER_1_SECRET]),
                LOCAL_DOMAIN,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AttestationError::UnknownAttester { .. }));

        let message = test_message(0, LOCAL_DOMAIN, 22);
        verifier
            .verify(
                &message,
                &attest(&message, &[&ATTESTER_2_SECRET]),
                LOCAL_DOMAIN,
            )
            .await
            .unwrap();
        assert!(fetches.load(Ordering::SeqCst) >= 2);
    }
}

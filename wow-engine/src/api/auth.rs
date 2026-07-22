//! Ed25519 request-body signature verification for internal service-to-service
//! traffic.
//!
//! Internal microservices (indexer, engine, ...) talk to each other over a VPC.
//! A compromised or misconfigured network must not let an attacker spoof
//! privileged requests to the engine. Every protected request therefore carries
//! an Ed25519 signature that the engine verifies *before* the request reaches a
//! handler.
//!
//! # Canonical signing string
//!
//! The caller signs the following newline-delimited byte string:
//!
//! ```text
//! {HTTP_METHOD}\n{URI_PATH}\n{UNIX_TIMESTAMP}\n{SHA256_HEX(body)}
//! ```
//!
//! Binding the method, path, timestamp, and a hash of the *exact* body means a
//! man-in-the-middle cannot tamper with any of them without invalidating the
//! signature (non-repudiation + integrity). The timestamp is additionally
//! range-checked to defeat replay attacks.
//!
//! # Wire format (request headers)
//!
//! | Header        | Value                                             |
//! |---------------|---------------------------------------------------|
//! | `X-Signature` | hex-encoded 64-byte Ed25519 signature             |
//! | `X-Timestamp` | Unix timestamp in **seconds** (signed integer)    |
//!
//! # Security posture: protected by default
//!
//! The middleware verifies *every* route except an explicit [`PUBLIC_PATHS`]
//! allowlist (health + public quoting). Adding a new sensitive endpoint is
//! therefore secure by default — it must be deliberately added to the allowlist
//! to become public, rather than deliberately added to a protected list to
//! become private.

use crate::error::AppError;
use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Response},
};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

/// Header carrying the hex-encoded Ed25519 signature.
pub const SIGNATURE_HEADER: &str = "x-signature";

/// Header carrying the Unix-seconds timestamp that was folded into the signature.
pub const TIMESTAMP_HEADER: &str = "x-timestamp";

/// Endpoints that are intentionally public and therefore skip verification.
///
/// Kept intentionally tiny: only unauthenticated health checks and the public
/// quoting pathfinder belong here. Everything else is protected by default.
pub const PUBLIC_PATHS: &[&str] = &["/api/v1/health", "/api/v1/quote"];

/// Maximum request body we are willing to buffer for hashing (1 MiB).
///
/// Bounds the work an unauthenticated caller can force us to do before the
/// signature is even checked.
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Returns `true` if `path` is on the public allowlist and must bypass
/// signature verification.
pub fn is_public_path(path: &str) -> bool {
    PUBLIC_PATHS.contains(&path)
}

/// Builds the canonical string that both the caller and the engine sign/verify.
///
/// The format is stable and part of the wire contract; do not reorder fields.
pub fn canonical_message(
    method: &str,
    path: &str,
    timestamp: &str,
    body_sha256_hex: &str,
) -> String {
    format!("{method}\n{path}\n{timestamp}\n{body_sha256_hex}")
}

/// Lowercase hex-encoded SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Verifies Ed25519 signatures against a single trusted public key.
///
/// Holds the trusted verifying key plus the replay window. Cheap to clone
/// (the key is 32 bytes), so it can be used directly as middleware state.
#[derive(Clone)]
pub struct SignatureVerifier {
    key: VerifyingKey,
    max_age_secs: i64,
}

impl SignatureVerifier {
    /// Default replay window: reject signatures whose timestamp is more than
    /// five minutes away from now (in either direction).
    pub const DEFAULT_MAX_AGE_SECS: i64 = 300;

    /// Builds a verifier from a hex-encoded 32-byte Ed25519 public key, using
    /// the [`DEFAULT_MAX_AGE_SECS`](Self::DEFAULT_MAX_AGE_SECS) replay window.
    pub fn from_hex_public_key(public_key_hex: &str) -> anyhow::Result<Self> {
        Self::from_hex_public_key_with_max_age(public_key_hex, Self::DEFAULT_MAX_AGE_SECS)
    }

    /// Builds a verifier from a hex-encoded 32-byte Ed25519 public key with an
    /// explicit replay window (seconds).
    pub fn from_hex_public_key_with_max_age(
        public_key_hex: &str,
        max_age_secs: i64,
    ) -> anyhow::Result<Self> {
        let raw = hex::decode(public_key_hex.trim())
            .map_err(|e| anyhow::anyhow!("signing public key is not valid hex: {e}"))?;
        let bytes: [u8; 32] = raw
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("signing public key must be 32 bytes (64 hex chars)"))?;
        let key = VerifyingKey::from_bytes(&bytes)
            .map_err(|e| anyhow::anyhow!("signing public key is not a valid Ed25519 key: {e}"))?;
        Ok(Self { key, max_age_secs })
    }

    /// Rejects a timestamp that is outside the `±max_age_secs` window around
    /// "now". An old timestamp is a replayed request; a far-future one is a
    /// misconfigured clock or a forgery attempt.
    fn check_timestamp(&self, timestamp: i64) -> Result<(), AppError> {
        let now = chrono::Utc::now().timestamp();
        let skew = now - timestamp;
        if skew > self.max_age_secs || skew < -self.max_age_secs {
            return Err(unauthorized(
                "request timestamp is outside the allowed window",
            ));
        }
        Ok(())
    }

    /// Verifies the signature covering `request` and returns the request with
    /// its body restored so downstream handlers can still read it.
    ///
    /// Callers must have already ruled out [`is_public_path`]; this always
    /// requires a valid signature.
    async fn verify(&self, request: Request) -> Result<Request, AppError> {
        let method = request.method().as_str().to_owned();
        let path = request.uri().path().to_owned();

        // Pull the auth headers out as owned strings before we consume `request`.
        let signature_hex = required_header(request.headers(), SIGNATURE_HEADER)?.to_owned();
        let timestamp_str = required_header(request.headers(), TIMESTAMP_HEADER)?.to_owned();

        // Enforce the replay window before doing any signature math.
        let timestamp: i64 = timestamp_str
            .parse()
            .map_err(|_| unauthorized("timestamp is not a valid integer"))?;
        self.check_timestamp(timestamp)?;

        // Decode the 64-byte signature.
        let sig_bytes = hex::decode(signature_hex.trim())
            .map_err(|_| unauthorized("signature is not valid hex"))?;
        let sig_array: [u8; 64] = sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| unauthorized("signature must be 64 bytes"))?;
        let signature = Signature::from_bytes(&sig_array);

        // Buffer the body so we can hash it, bounding total work.
        let (parts, body) = request.into_parts();
        let body_bytes = to_bytes(body, MAX_BODY_BYTES)
            .await
            .map_err(|_| unauthorized("request body could not be read"))?;

        let message = canonical_message(&method, &path, &timestamp_str, &sha256_hex(&body_bytes));

        // `verify_strict` rejects malleable/weak signatures, unlike `verify`.
        self.key
            .verify_strict(message.as_bytes(), &signature)
            .map_err(|_| unauthorized("signature verification failed"))?;

        Ok(Request::from_parts(parts, Body::from(body_bytes)))
    }
}

/// Axum/Tower middleware that enforces [`SignatureVerifier`] on every protected
/// route. Wire it up with [`axum::middleware::from_fn_with_state`].
///
/// Public paths ([`PUBLIC_PATHS`]) are passed straight through without touching
/// their body; everything else must present a valid signature or receives an
/// immediate `401 Unauthorized`.
pub async fn verify_signature(
    State(verifier): State<SignatureVerifier>,
    request: Request,
    next: Next,
) -> Response {
    if is_public_path(request.uri().path()) {
        return next.run(request).await;
    }

    match verifier.verify(request).await {
        Ok(request) => next.run(request).await,
        Err(err) => err.into_response(),
    }
}

/// Fetches a required header as `&str`, mapping absence/encoding errors to 401.
fn required_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, AppError> {
    headers
        .get(name)
        .ok_or_else(|| unauthorized(format!("missing {name} header")))?
        .to_str()
        .map_err(|_| unauthorized(format!("{name} header is not valid ASCII")))
}

fn unauthorized(msg: impl Into<String>) -> AppError {
    AppError::Unauthorized(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn test_signer() -> (SigningKey, SignatureVerifier) {
        // Deterministic keypair from a fixed seed keeps the test hermetic.
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
        let verifier = SignatureVerifier::from_hex_public_key(&public_key_hex).unwrap();
        (signing_key, verifier)
    }

    #[test]
    fn canonical_message_format_is_stable() {
        assert_eq!(
            canonical_message("POST", "/api/v1/execute-route", "1700000000", "deadbeef"),
            "POST\n/api/v1/execute-route\n1700000000\ndeadbeef"
        );
    }

    #[test]
    fn public_paths_bypass_but_sensitive_paths_do_not() {
        assert!(is_public_path("/api/v1/health"));
        assert!(is_public_path("/api/v1/quote"));
        assert!(!is_public_path("/api/v1/execute-route"));
        assert!(!is_public_path("/api/v1/anchor/withdraw"));
    }

    #[test]
    fn valid_signature_verifies() {
        let (signing_key, verifier) = test_signer();
        let body = br#"{"amount_in":100}"#;
        let ts = "1700000000";
        let message = canonical_message("POST", "/api/v1/execute-route", ts, &sha256_hex(body));
        let signature = signing_key.sign(message.as_bytes());

        assert!(verifier
            .key
            .verify_strict(message.as_bytes(), &signature)
            .is_ok());
    }

    #[test]
    fn single_byte_body_change_invalidates_signature() {
        let (signing_key, verifier) = test_signer();
        let body = br#"{"amount_in":100}"#;
        let ts = "1700000000";
        let message = canonical_message("POST", "/api/v1/execute-route", ts, &sha256_hex(body));
        let signature = signing_key.sign(message.as_bytes());

        // Flip exactly one bit of one byte of the body.
        let mut tampered = body.to_vec();
        tampered[0] ^= 0x01;
        let tampered_message =
            canonical_message("POST", "/api/v1/execute-route", ts, &sha256_hex(&tampered));

        assert!(verifier
            .key
            .verify_strict(tampered_message.as_bytes(), &signature)
            .is_err());
    }

    #[test]
    fn timestamp_window_blocks_replays_and_future_skew() {
        let (_signing_key, verifier) = test_signer();
        let now = chrono::Utc::now().timestamp();

        assert!(verifier.check_timestamp(now).is_ok());
        assert!(verifier.check_timestamp(now - 60).is_ok());
        // Older than the 5-minute window -> replay.
        assert!(verifier.check_timestamp(now - 301).is_err());
        // Too far in the future -> clock skew / forgery.
        assert!(verifier.check_timestamp(now + 301).is_err());
    }

    #[test]
    fn rejects_public_key_of_wrong_length() {
        assert!(SignatureVerifier::from_hex_public_key("abcd").is_err());
        assert!(SignatureVerifier::from_hex_public_key("not-hex").is_err());
    }
}

use crate::error::AppError;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResponse {
    pub transaction: String,
    pub network_passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub token: String,
}

const STELLAR_PUBLIC_NETWORK_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";
const STELLAR_TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";

const XDR_ACCOUNT_ED25519: u32 = 0;
const XDR_MEMO_NONE: u32 = 0;
const XDR_OP_MANAGE_DATA: u32 = 11;
const XDR_ENVELOPE_TYPE_TX: u32 = 2;
const XDR_TX_EXT_V0: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
struct AccountId(pub [u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq)]
enum MuxedAccount {
    Ed25519(AccountId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimeBounds {
    min_time: u64,
    max_time: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Memo {
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManageDataOp {
    key: String,
    value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Operation {
    source_account: Option<MuxedAccount>,
    body: OperationBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OperationBody {
    ManageData(ManageDataOp),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Transaction {
    source_account: MuxedAccount,
    fee: u32,
    seq_num: i64,
    time_bounds: Option<TimeBounds>,
    memo: Memo,
    operations: Vec<Operation>,
    ext: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecoratedSignature {
    hint: [u8; 4],
    signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TransactionEnvelope {
    tx: Transaction,
    signatures: Vec<DecoratedSignature>,
}

struct XdrReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> XdrReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn read_u32(&mut self) -> Result<u32, AppError> {
        if self.remaining() < 4 {
            return Err(bad_request("XDR: truncated u32"));
        }
        let bytes: [u8; 4] = self.data[self.pos..self.pos + 4].try_into().unwrap();
        self.pos += 4;
        Ok(u32::from_be_bytes(bytes))
    }

    #[allow(dead_code)]
    fn read_i32(&mut self) -> Result<i32, AppError> {
        if self.remaining() < 4 {
            return Err(bad_request("XDR: truncated i32"));
        }
        let bytes: [u8; 4] = self.data[self.pos..self.pos + 4].try_into().unwrap();
        self.pos += 4;
        Ok(i32::from_be_bytes(bytes))
    }

    fn read_i64(&mut self) -> Result<i64, AppError> {
        if self.remaining() < 8 {
            return Err(bad_request("XDR: truncated i64"));
        }
        let bytes: [u8; 8] = self.data[self.pos..self.pos + 8].try_into().unwrap();
        self.pos += 8;
        Ok(i64::from_be_bytes(bytes))
    }

    fn read_u64(&mut self) -> Result<u64, AppError> {
        if self.remaining() < 8 {
            return Err(bad_request("XDR: truncated u64"));
        }
        let bytes: [u8; 8] = self.data[self.pos..self.pos + 8].try_into().unwrap();
        self.pos += 8;
        Ok(u64::from_be_bytes(bytes))
    }

    fn read_fixed_bytes(&mut self, n: usize) -> Result<Vec<u8>, AppError> {
        if self.remaining() < n {
            return Err(bad_request("XDR: truncated fixed bytes"));
        }
        let v = self.data[self.pos..self.pos + n].to_vec();
        self.pos += n;
        Ok(v)
    }

    fn read_opaque(&mut self) -> Result<Vec<u8>, AppError> {
        let len = self.read_u32()? as usize;
        let mut data = self.read_fixed_bytes(len)?;
        let pad = (4 - (len % 4)) % 4;
        if pad > 0 {
            if self.remaining() < pad {
                return Err(bad_request("XDR: truncated opaque pad"));
            }
            self.pos += pad;
        }
        data.truncate(len);
        Ok(data)
    }

    fn read_string(&mut self) -> Result<String, AppError> {
        let bytes = self.read_opaque()?;
        String::from_utf8(bytes).map_err(|_| bad_request("XDR: invalid string"))
    }

    fn read_optional<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, AppError>,
    ) -> Result<Option<T>, AppError> {
        let present = self.read_u32()?;
        if present == 0 {
            Ok(None)
        } else if present == 1 {
            f(self).map(Some)
        } else {
            Err(bad_request("XDR: invalid optional discriminant"))
        }
    }

    fn read_vec<T>(
        &mut self,
        f: impl Fn(&mut Self) -> Result<T, AppError>,
    ) -> Result<Vec<T>, AppError> {
        let count = self.read_u32()? as usize;
        let mut v = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            v.push(f(self)?);
        }
        Ok(v)
    }
}

fn decode_account_id(reader: &mut XdrReader) -> Result<AccountId, AppError> {
    let type_ = reader.read_u32()?;
    if type_ != XDR_ACCOUNT_ED25519 {
        return Err(bad_request("XDR: unsupported account type"));
    }
    let bytes = reader.read_fixed_bytes(32)?;
    Ok(AccountId(bytes.try_into().unwrap()))
}

fn decode_muxed(reader: &mut XdrReader) -> Result<MuxedAccount, AppError> {
    let type_ = reader.read_u32()?;
    if type_ != XDR_ACCOUNT_ED25519 {
        return Err(bad_request("XDR: unsupported muxed account type"));
    }
    let id = decode_account_id(reader)?;
    Ok(MuxedAccount::Ed25519(id))
}

fn decode_timebounds(reader: &mut XdrReader) -> Result<TimeBounds, AppError> {
    Ok(TimeBounds {
        min_time: reader.read_u64()?,
        max_time: reader.read_u64()?,
    })
}

fn decode_memo(reader: &mut XdrReader) -> Result<Memo, AppError> {
    let type_ = reader.read_u32()?;
    if type_ != XDR_MEMO_NONE {
        return Err(bad_request(
            "XDR: unsupported memo type in SEP-10 challenge",
        ));
    }
    Ok(Memo::None)
}

fn decode_operation(reader: &mut XdrReader) -> Result<Operation, AppError> {
    let source_account = reader.read_optional(|r| decode_muxed(r))?;
    let body_type = reader.read_u32()?;
    if body_type != XDR_OP_MANAGE_DATA {
        return Err(bad_request(
            "XDR: SEP-10 challenge must contain only ManageData ops",
        ));
    }
    let key = reader.read_string()?;
    let value = reader.read_optional(|r| r.read_opaque())?;
    Ok(Operation {
        source_account,
        body: OperationBody::ManageData(ManageDataOp { key, value }),
    })
}

fn decode_transaction(reader: &mut XdrReader) -> Result<Transaction, AppError> {
    let source_account = decode_muxed(reader)?;
    let fee = reader.read_u32()?;
    let seq_num = reader.read_i64()?;
    let time_bounds = reader.read_optional(|r| decode_timebounds(r))?;
    let memo = decode_memo(reader)?;
    let operations = reader.read_vec(|r| decode_operation(r))?;
    let ext = reader.read_u32()?;
    if ext != XDR_TX_EXT_V0 {
        return Err(bad_request("XDR: unsupported transaction ext"));
    }
    Ok(Transaction {
        source_account,
        fee,
        seq_num,
        time_bounds,
        memo,
        operations,
        ext,
    })
}

fn decode_decorated_signature(reader: &mut XdrReader) -> Result<DecoratedSignature, AppError> {
    let hint_bytes = reader.read_fixed_bytes(4)?;
    let hint: [u8; 4] = hint_bytes.try_into().unwrap();
    let signature = reader.read_opaque()?;
    if signature.len() != 64 {
        return Err(bad_request("XDR: invalid signature length"));
    }
    Ok(DecoratedSignature { hint, signature })
}

fn decode_transaction_envelope(xdr_b64: &str) -> Result<TransactionEnvelope, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(xdr_b64.trim())
        .map_err(|_| bad_request("Invalid base64 transaction envelope"))?;
    let mut reader = XdrReader::new(&bytes);
    let env_type = reader.read_u32()?;
    if env_type != XDR_ENVELOPE_TYPE_TX {
        return Err(bad_request("XDR: only ENVELOPE_TYPE_TX is supported"));
    }
    let tx = decode_transaction(&mut reader)?;
    let signatures = reader.read_vec(|r| decode_decorated_signature(r))?;
    if reader.remaining() != 0 {
        return Err(bad_request("XDR: trailing bytes after envelope"));
    }
    Ok(TransactionEnvelope { tx, signatures })
}

struct XdrWriter {
    buf: Vec<u8>,
}

impl XdrWriter {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    fn write_i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    fn write_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    fn write_fixed_bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }

    fn write_opaque(&mut self, v: &[u8]) {
        self.write_u32(v.len() as u32);
        self.buf.extend_from_slice(v);
        let pad = (4 - (v.len() % 4)) % 4;
        for _ in 0..pad {
            self.buf.push(0);
        }
    }

    fn write_string(&mut self, s: &str) {
        self.write_opaque(s.as_bytes());
    }

    fn write_optional_present(&mut self) {
        self.write_u32(1);
    }

    fn write_optional_absent(&mut self) {
        self.write_u32(0);
    }
}

fn encode_account_id(w: &mut XdrWriter, id: &AccountId) {
    w.write_u32(XDR_ACCOUNT_ED25519);
    w.write_fixed_bytes(&id.0);
}

fn encode_muxed(w: &mut XdrWriter, m: &MuxedAccount) {
    match m {
        MuxedAccount::Ed25519(id) => {
            w.write_u32(XDR_ACCOUNT_ED25519);
            encode_account_id(w, id);
        }
    }
}

fn encode_timebounds(w: &mut XdrWriter, tb: &TimeBounds) {
    w.write_u64(tb.min_time);
    w.write_u64(tb.max_time);
}

fn encode_memo(w: &mut XdrWriter) {
    w.write_u32(XDR_MEMO_NONE);
}

fn encode_operation(w: &mut XdrWriter, op: &Operation) {
    match &op.source_account {
        Some(src) => {
            w.write_optional_present();
            encode_muxed(w, src);
        }
        None => w.write_optional_absent(),
    }
    w.write_u32(XDR_OP_MANAGE_DATA);
    match &op.body {
        OperationBody::ManageData(md) => {
            w.write_string(&md.key);
            match &md.value {
                Some(v) => {
                    w.write_optional_present();
                    w.write_opaque(v);
                }
                None => w.write_optional_absent(),
            }
        }
    }
}

fn encode_transaction(w: &mut XdrWriter, tx: &Transaction) {
    encode_muxed(w, &tx.source_account);
    w.write_u32(tx.fee);
    w.write_i64(tx.seq_num);
    match &tx.time_bounds {
        Some(tb) => {
            w.write_optional_present();
            encode_timebounds(w, tb);
        }
        None => w.write_optional_absent(),
    }
    encode_memo(w);
    w.write_u32(tx.operations.len() as u32);
    for op in &tx.operations {
        encode_operation(w, op);
    }
    w.write_u32(tx.ext);
}

fn encode_decorated_signature(w: &mut XdrWriter, ds: &DecoratedSignature) {
    w.write_fixed_bytes(&ds.hint);
    w.write_opaque(&ds.signature);
}

fn encode_transaction_envelope(env: &TransactionEnvelope) -> String {
    let mut w = XdrWriter::new();
    w.write_u32(XDR_ENVELOPE_TYPE_TX);
    encode_transaction(&mut w, &env.tx);
    w.write_u32(env.signatures.len() as u32);
    for sig in &env.signatures {
        encode_decorated_signature(&mut w, sig);
    }
    base64::engine::general_purpose::STANDARD.encode(&w.buf)
}

fn encode_transaction_signature_payload(network_id: &[u8; 32], tx: &Transaction) -> Vec<u8> {
    let mut w = XdrWriter::new();
    w.write_fixed_bytes(network_id);
    w.write_u32(XDR_ENVELOPE_TYPE_TX);
    encode_transaction(&mut w, tx);
    w.buf
}

fn network_id(passphrase: &str) -> [u8; 32] {
    let hash = Sha256::digest(passphrase.as_bytes());
    hash.into()
}

fn transaction_hash(passphrase: &str, tx: &Transaction) -> [u8; 32] {
    let net_id = network_id(passphrase);
    let payload = encode_transaction_signature_payload(&net_id, tx);
    let hash = Sha256::digest(payload);
    hash.into()
}

fn signature_hint(public_key: &[u8; 32]) -> [u8; 4] {
    [
        public_key[28],
        public_key[29],
        public_key[30],
        public_key[31],
    ]
}

fn muxed_account_bytes(m: &MuxedAccount) -> [u8; 32] {
    match m {
        MuxedAccount::Ed25519(id) => id.0,
    }
}

fn resolve_network_passphrase(response_passphrase: &Option<String>, anchor_domain: &str) -> String {
    if let Some(p) = response_passphrase {
        return p.clone();
    }
    if anchor_domain.ends_with(".test")
        || anchor_domain.starts_with("test")
        || anchor_domain.contains("test")
        || anchor_domain == "test.com"
    {
        STELLAR_TESTNET_PASSPHRASE.to_string()
    } else {
        STELLAR_PUBLIC_NETWORK_PASSPHRASE.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChallenge {
    pub anchor_account: [u8; 32],
    pub client_account: [u8; 32],
    pub min_time: u64,
    pub max_time: u64,
    pub seq_num: i64,
}

pub fn parse_and_validate_challenge(
    xdr_b64: &str,
    expected_client_account: &[u8; 32],
    network_passphrase: &str,
    now_unix_secs: i64,
    max_age_secs: i64,
    max_future_skew_secs: i64,
) -> Result<ParsedChallenge, AppError> {
    let env = decode_transaction_envelope(xdr_b64)?;
    let tx = &env.tx;

    if tx.seq_num != 0 {
        return Err(bad_request("SEP-10 challenge must have sequence number 0"));
    }

    let tb = tx
        .time_bounds
        .as_ref()
        .ok_or_else(|| bad_request("SEP-10 challenge must have time bounds"))?;

    if tb.min_time == 0 {
        return Err(bad_request("SEP-10 challenge minTime must be set"));
    }

    let min_time_i64 = tb.min_time as i64;
    let age = now_unix_secs.saturating_sub(min_time_i64);
    if age > max_age_secs {
        return Err(unauthorized("SEP-10 challenge has expired"));
    }
    let skew = min_time_i64.saturating_sub(now_unix_secs);
    if skew > max_future_skew_secs {
        return Err(unauthorized(
            "SEP-10 challenge minTime is too far in the future",
        ));
    }

    if tb.max_time != 0 && tb.max_time < tb.min_time {
        return Err(bad_request("SEP-10 challenge maxTime must be >= minTime"));
    }

    let anchor_account = muxed_account_bytes(&tx.source_account);

    if tx.operations.is_empty() {
        return Err(bad_request(
            "SEP-10 challenge must contain at least one ManageData op",
        ));
    }

    let mut found_client_account = None;
    for op in &tx.operations {
        let (_md_key, md_value, op_source) = match &op.body {
            OperationBody::ManageData(md) => (
                &md.key,
                &md.value,
                op.source_account
                    .as_ref()
                    .map(muxed_account_bytes)
                    .unwrap_or(anchor_account),
            ),
        };
        let op_source_arr = op_source;
        if op_source_arr == *expected_client_account {
            if let Some(value) = md_value {
                if value.len() == 48 || value.len() == 32 || value.len() == 64 {
                    found_client_account = Some(op_source_arr);
                }
            }
        }
    }

    if found_client_account.is_none() {
        for op in &tx.operations {
            let op_source = op
                .source_account
                .as_ref()
                .map(muxed_account_bytes)
                .unwrap_or(anchor_account);
            if op_source == *expected_client_account {
                found_client_account = Some(op_source);
            }
        }
    }

    let client_account = found_client_account.ok_or_else(|| {
        bad_request("SEP-10 challenge missing ManageData op with client account as source")
    })?;

    if client_account != *expected_client_account {
        return Err(bad_request("SEP-10 challenge client account mismatch"));
    }

    if env.signatures.is_empty() {
        return Err(bad_request(
            "SEP-10 challenge must have at least the anchor signature",
        ));
    }

    let tx_hash = transaction_hash(network_passphrase, tx);
    let mut has_valid_anchor_sig = false;
    for ds in &env.signatures {
        if ds.hint == signature_hint(&anchor_account) && ds.signature.len() == 64 {
            let sig_arr: [u8; 64] = ds.signature.as_slice().try_into().unwrap();
            let sig = Signature::from_bytes(&sig_arr);
            if let Ok(vk) = VerifyingKey::from_bytes(&anchor_account) {
                if vk.verify_strict(&tx_hash, &sig).is_ok() {
                    has_valid_anchor_sig = true;
                    break;
                }
            }
        }
    }
    if !has_valid_anchor_sig {
        return Err(unauthorized(
            "SEP-10 challenge missing valid anchor signature",
        ));
    }

    Ok(ParsedChallenge {
        anchor_account,
        client_account,
        min_time: tb.min_time,
        max_time: tb.max_time,
        seq_num: tx.seq_num,
    })
}

pub fn sign_challenge_transaction(
    xdr_b64: &str,
    signing_key: &SigningKey,
) -> Result<String, AppError> {
    let mut env = decode_transaction_envelope(xdr_b64)?;
    let public_key: [u8; 32] = signing_key.verifying_key().to_bytes();
    let hint = signature_hint(&public_key);
    let tx_hash = transaction_hash(STELLAR_PUBLIC_NETWORK_PASSPHRASE, &env.tx);
    let testnet_hash = transaction_hash(STELLAR_TESTNET_PASSPHRASE, &env.tx);
    let sig_public = signing_key.sign(&tx_hash);
    let sig_testnet = signing_key.sign(&testnet_hash);
    for net_hash in [tx_hash, testnet_hash].iter() {
        for ds in &env.signatures {
            if ds.hint == hint && ds.signature.len() == 64 {
                let sig_arr: [u8; 64] = ds.signature.as_slice().try_into().unwrap();
                let sig = Signature::from_bytes(&sig_arr);
                if let Ok(vk) = VerifyingKey::from_bytes(&public_key) {
                    if vk.verify_strict(net_hash, &sig).is_ok() {
                        return Ok(encode_transaction_envelope(&env));
                    }
                }
            }
        }
    }
    env.signatures.push(DecoratedSignature {
        hint,
        signature: sig_public.to_bytes().to_vec(),
    });
    let _ = sig_testnet;
    Ok(encode_transaction_envelope(&env))
}

fn public_key_from_stellar_account(account: &str) -> Result<[u8; 32], AppError> {
    let decoded = data_encoding::BASE32_NOPAD
        .decode(account.as_bytes())
        .map_err(|_| bad_request("Invalid Stellar account ID format"))?;
    if decoded.len() != 35 {
        return Err(bad_request("Invalid Stellar account ID length"));
    }
    let raw: [u8; 35] = decoded.try_into().unwrap();
    if raw[0] != 0x30 {
        return Err(bad_request("Invalid Stellar account ID version byte"));
    }
    let key_bytes: [u8; 32] = raw[1..33].try_into().unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&raw[0..33]);
    let checksum_full: [u8; 32] = hasher.finalize().into();
    let mut hasher2 = Sha256::new();
    hasher2.update(checksum_full);
    let checksum: [u8; 32] = hasher2.finalize().into();
    if checksum[0] != raw[33] || checksum[1] != raw[34] {
        return Err(bad_request("Invalid Stellar account ID checksum"));
    }
    Ok(key_bytes)
}

fn secret_key_from_stellar_seed(seed: &str) -> Result<SigningKey, AppError> {
    let decoded = data_encoding::BASE32_NOPAD
        .decode(seed.as_bytes())
        .map_err(|_| bad_request("Invalid Stellar secret seed format"))?;
    if decoded.len() != 35 {
        return Err(bad_request("Invalid Stellar secret seed length"));
    }
    let raw: [u8; 35] = decoded.try_into().unwrap();
    if raw[0] != 0x90 {
        return Err(bad_request("Invalid Stellar secret seed version byte"));
    }
    let seed_bytes: [u8; 32] = raw[1..33].try_into().unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&raw[0..33]);
    let checksum_full: [u8; 32] = hasher.finalize().into();
    let mut hasher2 = Sha256::new();
    hasher2.update(checksum_full);
    let checksum: [u8; 32] = hasher2.finalize().into();
    if checksum[0] != raw[33] || checksum[1] != raw[34] {
        return Err(bad_request("Invalid Stellar secret seed checksum"));
    }
    Ok(SigningKey::from_bytes(&seed_bytes))
}

pub struct Sep10Client {
    client: ClientWithMiddleware,
    token_cache: Arc<RwLock<HashMap<(String, String), String>>>,
    signing_keys: HashMap<String, SigningKey>,
    challenge_max_age_secs: i64,
    challenge_max_future_skew_secs: i64,
}

impl Sep10Client {
    pub fn from_config(
        signing_keys_entries: &[String],
        max_age_secs: i64,
        max_future_skew_secs: i64,
    ) -> Result<Self, anyhow::Error> {
        let mut keys = HashMap::new();
        for entry in signing_keys_entries {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let mut parts = entry.splitn(2, '=');
            let account = parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("SEP10_SIGNING_KEYS missing account"))?
                .trim()
                .to_string();
            let secret = parts
                .next()
                .ok_or_else(|| {
                    anyhow::anyhow!("SEP10_SIGNING_KEYS missing secret for account {account}")
                })?
                .trim()
                .to_string();
            let pk = public_key_from_stellar_account(&account)
                .map_err(|e| anyhow::anyhow!("SEP10_SIGNING_KEYS bad account {account}: {e}"))?;
            let sk = secret_key_from_stellar_seed(&secret)
                .map_err(|e| anyhow::anyhow!("SEP10_SIGNING_KEYS bad secret for {account}: {e}"))?;
            if sk.verifying_key().to_bytes() != pk {
                return Err(anyhow::anyhow!(
                    "SEP10_SIGNING_KEYS account {account} does not match its secret"
                ));
            }
            keys.insert(account, sk);
        }
        Ok(Self {
            client: crate::http_client::build_resilient_client()
                .expect("Failed to build resilient HTTP client"),
            token_cache: Arc::new(RwLock::new(HashMap::new())),
            signing_keys: keys,
            challenge_max_age_secs: max_age_secs,
            challenge_max_future_skew_secs: max_future_skew_secs,
        })
    }

    pub fn new() -> Self {
        Self::from_config(&[], 300, 60).expect("empty signing keys config")
    }

    pub fn with_test_key(account: &str, secret: &str) -> Self {
        let mut me = Self::new();
        let sk = secret_key_from_stellar_seed(secret).expect("test secret key");
        me.signing_keys.insert(account.to_string(), sk);
        me
    }

    pub async fn authenticate(
        &self,
        anchor_domain: &str,
        account: &str,
    ) -> Result<String, crate::error::AppError> {
        let cache_key = (account.to_string(), anchor_domain.to_string());

        {
            let cache = self.token_cache.read().await;
            if let Some(token) = cache.get(&cache_key) {
                return Ok(token.clone());
            }
        }

        let key = self.signing_keys.get(account).ok_or_else(|| {
            bad_request(format!(
                "No SEP-10 signing key configured for account {account}"
            ))
        })?;
        let account_pk_bytes: [u8; 32] = public_key_from_stellar_account(account)?;

        let challenge_url = format!("https://{}/auth?account={}", anchor_domain, account);
        let challenge: ChallengeResponse = self
            .client
            .get(&challenge_url)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?
            .json()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

        let passphrase = resolve_network_passphrase(&challenge.network_passphrase, anchor_domain);
        let now = chrono::Utc::now().timestamp();

        let _ = parse_and_validate_challenge(
            &challenge.transaction,
            &account_pk_bytes,
            &passphrase,
            now,
            self.challenge_max_age_secs,
            self.challenge_max_future_skew_secs,
        )?;

        let signed_tx = sign_challenge_transaction(&challenge.transaction, key)?;

        let token_req = serde_json::json!({
            "transaction": signed_tx
        });

        let token_resp: TokenResponse = self
            .client
            .post(&challenge_url)
            .json(&token_req)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?
            .json()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

        {
            let mut cache = self.token_cache.write().await;
            cache.insert(cache_key, token_resp.token.clone());
        }

        Ok(token_resp.token)
    }
}

impl Default for Sep10Client {
    fn default() -> Self {
        Self::new()
    }
}

fn bad_request(msg: impl Into<String>) -> AppError {
    AppError::BadRequest(msg.into())
}

fn unauthorized(msg: impl Into<String>) -> AppError {
    AppError::Unauthorized(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn deterministic_keypair_from_seed(seed: u8) -> (SigningKey, [u8; 32]) {
        let seed_bytes = [seed; 32];
        let sk = SigningKey::from_bytes(&seed_bytes);
        let pk = sk.verifying_key().to_bytes();
        (sk, pk)
    }

    fn build_test_challenge(
        anchor_sk: &SigningKey,
        client_pk: &[u8; 32],
        min_time_offset: i64,
        max_time: u64,
    ) -> (TransactionEnvelope, String) {
        let anchor_pk = anchor_sk.verifying_key().to_bytes();
        let now = chrono::Utc::now().timestamp();
        let min_time = (now + min_time_offset) as u64;
        let nonce = [42u8; 48];
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(AccountId(anchor_pk)),
            fee: 100,
            seq_num: 0,
            time_bounds: Some(TimeBounds { min_time, max_time }),
            memo: Memo::None,
            operations: vec![Operation {
                source_account: Some(MuxedAccount::Ed25519(AccountId(*client_pk))),
                body: OperationBody::ManageData(ManageDataOp {
                    key: "test.com auth".to_string(),
                    value: Some(nonce.to_vec()),
                }),
            }],
            ext: XDR_TX_EXT_V0,
        };
        let tx_hash_pub = transaction_hash(STELLAR_PUBLIC_NETWORK_PASSPHRASE, &tx);
        let anchor_sig = anchor_sk.sign(&tx_hash_pub);
        let env = TransactionEnvelope {
            tx,
            signatures: vec![DecoratedSignature {
                hint: signature_hint(&anchor_pk),
                signature: anchor_sig.to_bytes().to_vec(),
            }],
        };
        let xdr = encode_transaction_envelope(&env);
        (env, xdr)
    }

    #[test]
    fn valid_challenge_parses_and_verifies() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(1);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(2);
        let now = chrono::Utc::now().timestamp();
        let (_env, xdr) = build_test_challenge(&anchor_sk, &client_pk, -30, (now as u64) + 300);
        let parsed = parse_and_validate_challenge(
            &xdr,
            &client_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap();
        assert_eq!(parsed.client_account, client_pk);
        assert_eq!(parsed.seq_num, 0);
    }

    #[test]
    fn expired_challenge_is_rejected() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(3);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(4);
        let now = chrono::Utc::now().timestamp();
        let (_env, xdr) = build_test_challenge(&anchor_sk, &client_pk, -500, (now as u64) - 300);
        let err = parse_and_validate_challenge(
            &xdr,
            &client_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Unauthorized(_)));
    }

    #[test]
    fn too_future_challenge_is_rejected() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(5);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(6);
        let now = chrono::Utc::now().timestamp();
        let (_env, xdr) = build_test_challenge(&anchor_sk, &client_pk, 500, (now as u64) + 1000);
        let err = parse_and_validate_challenge(
            &xdr,
            &client_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Unauthorized(_)));
    }

    #[test]
    fn challenge_with_nonzero_seqnum_is_rejected() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(7);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(8);
        let now = chrono::Utc::now().timestamp();
        let (mut env, _xdr) = build_test_challenge(&anchor_sk, &client_pk, -10, (now as u64) + 300);
        env.tx.seq_num = 123;
        let anchor_pk = anchor_sk.verifying_key().to_bytes();
        let tx_hash = transaction_hash(STELLAR_PUBLIC_NETWORK_PASSPHRASE, &env.tx);
        let sig = anchor_sk.sign(&tx_hash);
        env.signatures = vec![DecoratedSignature {
            hint: signature_hint(&anchor_pk),
            signature: sig.to_bytes().to_vec(),
        }];
        let xdr = encode_transaction_envelope(&env);
        let err = parse_and_validate_challenge(
            &xdr,
            &client_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn challenge_missing_timebounds_is_rejected() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(9);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(10);
        let now = chrono::Utc::now().timestamp();
        let anchor_pk = anchor_sk.verifying_key().to_bytes();
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(AccountId(anchor_pk)),
            fee: 100,
            seq_num: 0,
            time_bounds: None,
            memo: Memo::None,
            operations: vec![Operation {
                source_account: Some(MuxedAccount::Ed25519(AccountId(client_pk))),
                body: OperationBody::ManageData(ManageDataOp {
                    key: "test.com auth".to_string(),
                    value: Some(vec![42u8; 48]),
                }),
            }],
            ext: XDR_TX_EXT_V0,
        };
        let tx_hash = transaction_hash(STELLAR_PUBLIC_NETWORK_PASSPHRASE, &tx);
        let sig = anchor_sk.sign(&tx_hash);
        let env = TransactionEnvelope {
            tx,
            signatures: vec![DecoratedSignature {
                hint: signature_hint(&anchor_pk),
                signature: sig.to_bytes().to_vec(),
            }],
        };
        let xdr = encode_transaction_envelope(&env);
        let err = parse_and_validate_challenge(
            &xdr,
            &client_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn challenge_without_anchor_signature_is_rejected() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(11);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(12);
        let now = chrono::Utc::now().timestamp();
        let (mut env, _xdr) = build_test_challenge(&anchor_sk, &client_pk, -10, (now as u64) + 300);
        env.signatures.clear();
        let xdr = encode_transaction_envelope(&env);
        let err = parse_and_validate_challenge(
            &xdr,
            &client_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn challenge_with_bad_anchor_signature_is_rejected() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(13);
        let (attacker_sk, _attacker_pk) = deterministic_keypair_from_seed(14);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(15);
        let now = chrono::Utc::now().timestamp();
        let (mut env, _xdr) = build_test_challenge(&anchor_sk, &client_pk, -10, (now as u64) + 300);
        let anchor_pk = anchor_sk.verifying_key().to_bytes();
        let wrong_sig = attacker_sk.sign(&[0u8; 32]);
        env.signatures = vec![DecoratedSignature {
            hint: signature_hint(&anchor_pk),
            signature: wrong_sig.to_bytes().to_vec(),
        }];
        let xdr = encode_transaction_envelope(&env);
        let err = parse_and_validate_challenge(
            &xdr,
            &client_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Unauthorized(_)));
    }

    #[test]
    fn challenge_with_wrong_client_account_is_rejected() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(16);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(17);
        let (_, other_pk) = deterministic_keypair_from_seed(18);
        let now = chrono::Utc::now().timestamp();
        let (_env, xdr) = build_test_challenge(&anchor_sk, &client_pk, -10, (now as u64) + 300);
        let err = parse_and_validate_challenge(
            &xdr,
            &other_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn sign_challenge_appends_valid_client_signature() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(19);
        let (client_sk, client_pk) = deterministic_keypair_from_seed(20);
        let now = chrono::Utc::now().timestamp();
        let (_env, xdr) = build_test_challenge(&anchor_sk, &client_pk, -10, (now as u64) + 300);
        let signed = sign_challenge_transaction(&xdr, &client_sk).unwrap();
        let signed_env = decode_transaction_envelope(&signed).unwrap();
        assert!(signed_env.signatures.len() >= 2);
        let client_hint = signature_hint(&client_pk);
        let client_sig = signed_env
            .signatures
            .iter()
            .find(|ds| ds.hint == client_hint)
            .expect("client signature appended");
        let tx_hash = transaction_hash(STELLAR_PUBLIC_NETWORK_PASSPHRASE, &signed_env.tx);
        let sig_bytes: [u8; 64] = client_sig.signature.as_slice().try_into().unwrap();
        let sig = Signature::from_bytes(&sig_bytes);
        client_sk
            .verifying_key()
            .verify_strict(&tx_hash, &sig)
            .unwrap();
    }

    #[test]
    fn xdr_round_trip_is_lossless() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(21);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(22);
        let now = chrono::Utc::now().timestamp();
        let (env, xdr) = build_test_challenge(&anchor_sk, &client_pk, -5, (now as u64) + 300);
        let decoded = decode_transaction_envelope(&xdr).unwrap();
        assert_eq!(decoded, env);
        let reencoded = encode_transaction_envelope(&decoded);
        assert_eq!(reencoded, xdr);
    }

    #[test]
    fn invalid_b64_is_rejected() {
        let err = decode_transaction_envelope("not-valid-xdr@@@").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn missing_client_op_source_is_rejected() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(23);
        let anchor_pk = anchor_sk.verifying_key().to_bytes();
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(24);
        let now = chrono::Utc::now().timestamp();
        let min_time = (now - 10) as u64;
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(AccountId(anchor_pk)),
            fee: 100,
            seq_num: 0,
            time_bounds: Some(TimeBounds {
                min_time,
                max_time: (now as u64) + 300,
            }),
            memo: Memo::None,
            operations: vec![Operation {
                source_account: None,
                body: OperationBody::ManageData(ManageDataOp {
                    key: "test.com auth".to_string(),
                    value: Some(vec![42u8; 48]),
                }),
            }],
            ext: XDR_TX_EXT_V0,
        };
        let tx_hash = transaction_hash(STELLAR_PUBLIC_NETWORK_PASSPHRASE, &tx);
        let sig = anchor_sk.sign(&tx_hash);
        let env = TransactionEnvelope {
            tx,
            signatures: vec![DecoratedSignature {
                hint: signature_hint(&anchor_pk),
                signature: sig.to_bytes().to_vec(),
            }],
        };
        let xdr = encode_transaction_envelope(&env);
        let err = parse_and_validate_challenge(
            &xdr,
            &client_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn stellar_account_id_round_trip() {
        let (_sk, pk) = deterministic_keypair_from_seed(25);
        let mut raw = [0u8; 35];
        raw[0] = 0x30;
        raw[1..33].copy_from_slice(&pk);
        let mut hasher = Sha256::new();
        hasher.update(&raw[0..33]);
        let c1: [u8; 32] = hasher.finalize().into();
        let mut hasher2 = Sha256::new();
        hasher2.update(c1);
        let c2: [u8; 32] = hasher2.finalize().into();
        raw[33] = c2[0];
        raw[34] = c2[1];
        let s = data_encoding::BASE32_NOPAD.encode(&raw);
        let decoded = public_key_from_stellar_account(&s).unwrap();
        assert_eq!(decoded, pk);
    }

    #[test]
    fn stellar_secret_seed_round_trip() {
        let seed = [77u8; 32];
        let sk_orig = SigningKey::from_bytes(&seed);
        let mut raw = [0u8; 35];
        raw[0] = 0x90;
        raw[1..33].copy_from_slice(&seed);
        let mut hasher = Sha256::new();
        hasher.update(&raw[0..33]);
        let c1: [u8; 32] = hasher.finalize().into();
        let mut hasher2 = Sha256::new();
        hasher2.update(c1);
        let c2: [u8; 32] = hasher2.finalize().into();
        raw[33] = c2[0];
        raw[34] = c2[1];
        let s = data_encoding::BASE32_NOPAD.encode(&raw);
        let sk = secret_key_from_stellar_seed(&s).unwrap();
        assert_eq!(sk.to_bytes(), sk_orig.to_bytes());
    }

    #[test]
    fn from_config_rejects_mismatched_account_and_secret() {
        let (sk, pk) = deterministic_keypair_from_seed(26);
        let mut raw_account = [0u8; 35];
        raw_account[0] = 0x30;
        raw_account[1..33].copy_from_slice(&[99u8; 32]);
        let mut h1 = Sha256::new();
        h1.update(&raw_account[0..33]);
        let c1: [u8; 32] = h1.finalize().into();
        let mut h2 = Sha256::new();
        h2.update(c1);
        let c2: [u8; 32] = h2.finalize().into();
        raw_account[33] = c2[0];
        raw_account[34] = c2[1];
        let account_str = data_encoding::BASE32_NOPAD.encode(&raw_account);

        let mut raw_seed = [0u8; 35];
        raw_seed[0] = 0x90;
        raw_seed[1..33].copy_from_slice(&sk.to_bytes());
        let mut hs1 = Sha256::new();
        hs1.update(&raw_seed[0..33]);
        let sc1: [u8; 32] = hs1.finalize().into();
        let mut hs2 = Sha256::new();
        hs2.update(sc1);
        let sc2: [u8; 32] = hs2.finalize().into();
        raw_seed[33] = sc2[0];
        raw_seed[34] = sc2[1];
        let secret_str = data_encoding::BASE32_NOPAD.encode(&raw_seed);

        let entry = format!("{account_str}={secret_str}");
        let err = match Sep10Client::from_config(&[entry], 300, 60) {
            Ok(_) => panic!("expected from_config to reject mismatched account/secret"),
            Err(e) => e,
        };
        let _ = pk;
        assert!(err.to_string().contains("does not match"));
    }

    #[test]
    fn parse_and_validate_handles_min_time_zero() {
        let (anchor_sk, _anchor_pk) = deterministic_keypair_from_seed(27);
        let (_client_sk, client_pk) = deterministic_keypair_from_seed(28);
        let now = chrono::Utc::now().timestamp();
        let anchor_pk = anchor_sk.verifying_key().to_bytes();
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(AccountId(anchor_pk)),
            fee: 100,
            seq_num: 0,
            time_bounds: Some(TimeBounds {
                min_time: 0,
                max_time: (now as u64) + 300,
            }),
            memo: Memo::None,
            operations: vec![Operation {
                source_account: Some(MuxedAccount::Ed25519(AccountId(client_pk))),
                body: OperationBody::ManageData(ManageDataOp {
                    key: "test.com auth".to_string(),
                    value: Some(vec![42u8; 48]),
                }),
            }],
            ext: XDR_TX_EXT_V0,
        };
        let tx_hash = transaction_hash(STELLAR_PUBLIC_NETWORK_PASSPHRASE, &tx);
        let sig = anchor_sk.sign(&tx_hash);
        let env = TransactionEnvelope {
            tx,
            signatures: vec![DecoratedSignature {
                hint: signature_hint(&anchor_pk),
                signature: sig.to_bytes().to_vec(),
            }],
        };
        let xdr = encode_transaction_envelope(&env);
        let err = parse_and_validate_challenge(
            &xdr,
            &client_pk,
            STELLAR_PUBLIC_NETWORK_PASSPHRASE,
            now,
            300,
            60,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }
}

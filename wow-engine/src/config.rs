use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;

/// Central configuration for the entire wow-engine.
///
/// Every environment-specific value that varies between staging and production
/// lives here, loaded once at startup from environment variables via
/// [`envy::from_env`]. Components receive `Arc<AppConfig>` instead of reaching
/// for their own hardcoded constants.
#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub database_url: Option<String>,
    /// Hex-encoded 32-byte Ed25519 public key of trusted internal callers.
    ///
    /// When set, all non-public endpoints require a valid `X-Signature`.
    /// When unset, internal request-signature verification is disabled — safe
    /// only for local development.
    #[serde(default)]
    pub signing_public_key: Option<String>,
    /// Upper bound, in seconds, on how long any single HTTP request may run
    /// before the server aborts it and returns `408 Request Timeout`.
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    /// Connection string for the Redis instance used to broadcast cache
    /// invalidations across nodes (e.g. `redis://localhost:6379`).
    ///
    /// When unset, the engine runs in single-node mode: it never publishes or
    /// subscribes to invalidation messages and relies purely on local cache
    /// TTLs.
    #[serde(default)]
    pub redis_url: Option<String>,
    /// JSON-RPC endpoint used to read Circle CCTP attester keys on-chain.
    #[serde(default = "default_eth_rpc_url")]
    pub eth_rpc_url: String,
    /// Address of Circle's `MessageTransmitter` contract on the source chain.
    #[serde(default = "default_cctp_message_transmitter")]
    pub cctp_message_transmitter: String,
    /// Path of the append-only log recording consumed CCTP nonces, so
    /// replay protection survives restarts and redeploys.
    #[serde(default = "default_cctp_nonce_store_path")]
    pub cctp_nonce_store_path: String,

    // ── Gas Oracle ──────────────────────────────────────────────────
    /// API key for Etherscan gas-tracker requests. When set, appended as
    /// `&apikey=<key>` to the Etherscan gastracker endpoint to authenticate
    /// outbound requests and avoid rate-limiting.
    #[serde(default)]
    pub etherscan_api_key: Option<String>,
    /// API key for Arbiscan gas-tracker requests.
    #[serde(default)]
    pub arbiscan_api_key: Option<String>,

    // ── Anchor & Frontend Allowlists ────────────────────────────────
    /// Comma-separated list of anchor domains the engine is permitted to
    /// interact with (e.g. `"localhost,anchor.stellar.org"`).
    ///
    /// Parsed from the `ALLOWED_ANCHOR_DOMAINS` env var as a comma-separated
    /// string. When non-empty, deposit/withdraw/quote requests referencing an
    /// anchor domain not in this list are rejected at the API layer.
    /// Explicit allowlist of allowed anchor domains to prevent SSRF
    /// vulnerabilities. Parsed from the `ALLOWED_ANCHOR_DOMAINS` env var as a
    /// comma-separated string. When non-empty, deposit/withdraw/quote requests
    /// referencing an anchor domain not in this list are rejected at the API
    /// layer.
    #[serde(default = "default_allowed_anchor_domains")]
    pub allowed_anchor_domains: HashSet<String>,

    /// Comma-separated list of allowed CORS origins (e.g.
    /// `"https://app.example.com,http://localhost:3000"`).
    ///
    /// When non-empty, only requests from these origins are permitted.
    /// When empty, CORS is fully permissive (local-dev mode).
    #[serde(default)]
    pub allowed_cors_origins: Vec<String>,

    // ── Rate limiting ───────────────────────────────────────────────
    /// Per-IP request budget, per 60-second window, applied to every route.
    #[serde(default = "default_rate_limit_global_per_minute")]
    pub rate_limit_global_per_minute: u32,
    /// Per-IP request budget, per 60-second window, applied specifically to
    /// `/api/v1/quote` on top of the global budget above — it runs a
    /// non-trivial pathfinding search per request, so it gets a stricter
    /// limit.
    #[serde(default = "default_rate_limit_quote_per_minute")]
    pub rate_limit_quote_per_minute: u32,
}

fn default_port() -> u16 {
    8080
}

fn default_request_timeout_secs() -> u64 {
    30
}

fn default_eth_rpc_url() -> String {
    "https://ethereum-rpc.publicnode.com".to_string()
}

fn default_cctp_message_transmitter() -> String {
    // Circle MessageTransmitter on Ethereum mainnet.
    "0x0a992d191deec32afe36203ad87d7d289a738f81".to_string()
}

fn default_cctp_nonce_store_path() -> String {
    "data/cctp_consumed_nonces.log".to_string()
}

fn default_rate_limit_global_per_minute() -> u32 {
    300
}

fn default_rate_limit_quote_per_minute() -> u32 {
    30
}

fn default_allowed_anchor_domains() -> HashSet<String> {
    [
        "testanchor.stellar.org",
        "lobstr.co",
        "anchor.mykuma.io",
        "test.com",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            database_url: None,
            signing_public_key: None,
            request_timeout_secs: default_request_timeout_secs(),
            redis_url: None,
            eth_rpc_url: default_eth_rpc_url(),
            cctp_message_transmitter: default_cctp_message_transmitter(),
            cctp_nonce_store_path: default_cctp_nonce_store_path(),
            etherscan_api_key: None,
            arbiscan_api_key: None,
            allowed_anchor_domains: default_allowed_anchor_domains(),
            allowed_cors_origins: Vec::new(),
            rate_limit_global_per_minute: default_rate_limit_global_per_minute(),
            rate_limit_quote_per_minute: default_rate_limit_quote_per_minute(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self, envy::Error> {
        envy::from_env::<AppConfig>()
    }

    pub fn get_database_url(&self) -> anyhow::Result<String> {
        self.database_url.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "DATABASE_URL environment variable not set. \
                 Example: postgres://postgres:postgres@localhost/wow_engine"
            )
        })
    }

    /// Wraps `self` in an `Arc` for cheap cloning across async tasks.
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

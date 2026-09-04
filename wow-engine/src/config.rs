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
    /// Stellar Horizon endpoint used to read wallet balances and payments.
    #[serde(default = "default_stellar_horizon_url")]
    pub stellar_horizon_url: String,

    // ── Gas Oracle ──────────────────────────────────────────────────
    /// API key for Etherscan gas-tracker requests. When set, appended as
    /// `&apikey=<key>` to the Etherscan gastracker endpoint to authenticate
    /// outbound requests and avoid rate-limiting.
    #[serde(default)]
    pub etherscan_api_key: Option<String>,
    /// API key for Arbiscan gas-tracker requests.
    #[serde(default)]
    pub arbiscan_api_key: Option<String>,

    // ── Price Oracle ────────────────────────────────────────────────
    /// API key for CoinGecko price lookups, sent as the `x-cg-demo-api-key`
    /// header. When unset, requests are sent unauthenticated against
    /// CoinGecko's public rate limit, same as the Etherscan/Arbiscan keys
    /// above.
    #[serde(default)]
    pub coingecko_api_key: Option<String>,

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
    /// Only requests from these origins receive `Access-Control-Allow-Origin`
    /// approval — there is no permissive fallback. Defaults to the web-app's
    /// local dev server so `cargo run` works out of the box; override for
    /// staging/prod with the real frontend origins.
    #[serde(default = "default_allowed_cors_origins")]
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
    /// Whether the engine is deployed behind a reverse proxy/load balancer
    /// that can be trusted to set `X-Forwarded-For` correctly.
    ///
    /// Defaults to `false`: an end client can set any `X-Forwarded-For`
    /// value it likes, so trusting it unconditionally would let anyone
    /// spoof their way around per-IP rate limiting. Rate limiting keys on
    /// the real TCP peer address unless this is explicitly set to `true`
    /// *and* the deployment topology actually guarantees that header is
    /// overwritten/stripped by a trusted proxy before reaching this engine.
    #[serde(default)]
    pub trust_proxy_headers: bool,

    // ── Mempool Front-Running Monitor ───────────────────────────────
    /// WSS endpoint the mempool listener connects to for real-time
    /// pending-transaction visibility on Ethereum mainnet, used to detect
    /// front-running/sandwich activity against pools our routes trade
    /// through. When unset, the listener does not start and routing
    /// behaves exactly as it did before this monitor existed.
    ///
    /// Must support Alchemy's `alchemy_pendingTransactions` subscription
    /// (address-filtered, full-transaction pending feed) — that server-side
    /// filtering is what keeps the listener from decoding the entire
    /// public mempool. This is an Alchemy-specific extension, not part of
    /// the standard `eth_subscribe("newPendingTransactions")` every
    /// provider implements, so a plain Infura (or other non-Alchemy)
    /// endpoint here will have its subscription request rejected — the
    /// listener will reconnect and log the rejection on every attempt
    /// rather than silently doing nothing, but it will never see a
    /// pending transaction. See `mempool::listener` for the reconnect/log
    /// behavior.
    #[serde(default)]
    pub mempool_wss_url: Option<String>,
    /// Extra contract addresses (DEX routers, the deBridge gateway, etc.)
    /// the mempool listener should watch, beyond `cctp_message_transmitter`
    /// which is always included. A pending call decodes into a *pool* (and
    /// so can feed sandwich detection) only for the DEX-router swap
    /// signatures this module knows how to parse — routers you want
    /// covered must be listed here. Comma-separated in the
    /// `MEMPOOL_WATCHED_CONTRACTS` env var.
    #[serde(default)]
    pub mempool_watched_contracts: Vec<String>,
    /// Comma-separated list of `STELLAR_ACCOUNT=SECRET_KEY` pairs used to sign
    /// SEP-10 challenges for the listed Stellar accounts.
    ///
    /// Example: `"GABCD...=SABCD...,GXYZZY...=SXYZZY..."`
    ///
    /// Only accounts with a configured secret can authenticate against anchors
    /// via SEP-10; calls to [`Sep10Client::authenticate`] referencing any other
    /// account are rejected with `BadRequest`. Keep empty for deployments that
    /// never initiate SEP-10 flows.
    #[serde(default)]
    pub sep10_signing_keys: Vec<String>,
    /// Maximum age, in seconds, that a SEP-10 challenge transaction's
    /// `timeBounds.minTime` may predate the current wall clock before we
    /// refuse to sign it. Defeats replay of recently-captured challenges.
    #[serde(default = "default_sep10_challenge_max_age_secs")]
    pub sep10_challenge_max_age_secs: i64,
    /// Maximum future skew, in seconds, that a SEP-10 challenge's
    /// `timeBounds.minTime` may be ahead of the current wall clock. Catches
    /// accidental clock-skew between the anchor and this engine.
    #[serde(default = "default_sep10_challenge_max_future_skew_secs")]
    pub sep10_challenge_max_future_skew_secs: i64,
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

fn default_allowed_cors_origins() -> Vec<String> {
    // The web-app's Vite dev server (see web-app/package.json). Deployments
    // must set ALLOWED_CORS_ORIGINS explicitly for staging/prod.
    vec!["http://localhost:5173".to_string()]
}

fn default_rate_limit_global_per_minute() -> u32 {
    300
}

fn default_rate_limit_quote_per_minute() -> u32 {
    30
}

fn default_stellar_horizon_url() -> String {
    "https://horizon-testnet.stellar.org".to_string()
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

fn default_sep10_challenge_max_age_secs() -> i64 {
    300
}

fn default_sep10_challenge_max_future_skew_secs() -> i64 {
    60
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
            stellar_horizon_url: default_stellar_horizon_url(),
            etherscan_api_key: None,
            arbiscan_api_key: None,
            coingecko_api_key: None,
            allowed_anchor_domains: default_allowed_anchor_domains(),
            allowed_cors_origins: default_allowed_cors_origins(),
            rate_limit_global_per_minute: default_rate_limit_global_per_minute(),
            rate_limit_quote_per_minute: default_rate_limit_quote_per_minute(),
            trust_proxy_headers: false,
            mempool_wss_url: None,
            mempool_watched_contracts: Vec::new(),
            sep10_signing_keys: Vec::new(),
            sep10_challenge_max_age_secs: default_sep10_challenge_max_age_secs(),
            sep10_challenge_max_future_skew_secs: default_sep10_challenge_max_future_skew_secs(),
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

    /// Every contract address the mempool listener should subscribe to,
    /// lowercased and deduplicated: `cctp_message_transmitter` plus any
    /// operator-configured `mempool_watched_contracts`.
    pub fn watched_mempool_contracts(&self) -> Vec<String> {
        let mut set: HashSet<String> = self
            .mempool_watched_contracts
            .iter()
            .map(|a| a.to_lowercase())
            .collect();
        set.insert(self.cctp_message_transmitter.to_lowercase());
        let mut contracts: Vec<String> = set.into_iter().collect();
        contracts.sort();
        contracts
    }
}

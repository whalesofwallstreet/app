use serde::Deserialize;

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
    ///
    /// This is the outermost backstop against a hung downstream dependency
    /// pinning a request (and its resources) open indefinitely. Individual
    /// dependencies enforce their own, tighter timeouts via the resilience
    /// layer; this guarantees a request can never outlive it.
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            database_url: None,
            signing_public_key: None,
            request_timeout_secs: default_request_timeout_secs(),
            eth_rpc_url: default_eth_rpc_url(),
            cctp_message_transmitter: default_cctp_message_transmitter(),
            cctp_nonce_store_path: default_cctp_nonce_store_path(),
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
}

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
}

fn default_port() -> u16 {
    8080
}

fn default_request_timeout_secs() -> u64 {
    30
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

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
}

fn default_port() -> u16 {
    8080
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

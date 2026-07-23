use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    #[serde(default = "default_port")]
    pub port: u16,
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
}

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Chain {
    Ethereum,
    Solana,
    Arbitrum,
    Stellar,
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeQuote {
    pub provider: String,
    pub source_chain: Chain,
    pub dest_chain: Chain,
    pub source_asset: String,
    pub dest_asset: String,
    pub amount_in: u64,
    pub amount_out: u64,
    pub estimated_fee_usd: f64,
    pub duration_seconds: u64,
    pub execution_payload: Option<String>,
}

pub trait BridgeProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn get_quote(
        &self,
        source_chain: Chain,
        dest_chain: Chain,
        source_asset: &str,
        dest_asset: &str,
        amount_in: u64,
    ) -> impl std::future::Future<Output = Result<BridgeQuote, anyhow::Error>> + Send;
}

pub mod attestation;
pub mod cctp;
pub mod debridge;
pub mod gas_oracle;

use crate::bridge::Chain;
use moka::future::Cache;
use reqwest::Client;
use std::time::Duration;
use tracing::{info, warn};
use serde_json::Value;

pub struct GasOracle {
    cache: Cache<Chain, f64>,
    client: Client,
}

impl GasOracle {
    pub fn new() -> Self {
        // Cache with 60 seconds TTL
        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(60))
            .build();
            
        // Fix #2: Enforce a strict HTTP timeout of 3 seconds
        let client = Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .expect("Failed to build HTTP client for GasOracle");

        Self {
            cache,
            client,
        }
    }

    pub async fn estimate_gas_fee_usd(&self, chain: Chain) -> f64 {
        // Fix #1: Use try_get_with to coalesce concurrent calls and fix Cache Stampede
        let fee = self.cache.try_get_with(chain, async {
            self.fetch_from_api(chain).await
        }).await;

        match fee {
            Ok(val) => val,
            Err(e) => {
                warn!("Failed to fetch gas fee for {:?} from oracle: {}. Using fallback.", chain, e);
                Self::fallback_fee(chain)
            }
        }
    }

    async fn fetch_from_api(&self, chain: Chain) -> Result<f64, std::sync::Arc<anyhow::Error>> {
        info!("Fetching real-time gas fee from external REST API for {:?}", chain);
        // Fix #3: Actual REST integrations for EVM chains
        let result = match chain {
            Chain::Ethereum => self.fetch_etherscan().await,
            Chain::Arbitrum => self.fetch_arbiscan().await,
            // For Solana/Stellar, there are no simple unauthenticated gas APIs, returning a safe fallback
            _ => Ok(Self::fallback_fee(chain)),
        };
        
        result.map_err(|e| std::sync::Arc::new(e))
    }

    async fn fetch_etherscan(&self) -> Result<f64, anyhow::Error> {
        let url = "https://api.etherscan.io/api?module=gastracker&action=gasoracle";
        let resp: Value = self.client.get(url).send().await?.json().await?;
        
        // Etherscan returns "ProposeGasPrice" in Gwei
        if let Some(price_str) = resp.get("result").and_then(|r| r.get("ProposeGasPrice")).and_then(|p| p.as_str()) {
            let gwei: f64 = price_str.parse()?;
            // Assume 150,000 gas limit for a bridge tx, and $3000 per ETH
            // Fee (USD) = gas_limit * gas_price_gwei * 10^-9 * eth_price_usd
            let fee_usd = 150_000.0 * gwei * 1e-9 * 3000.0;
            return Ok(fee_usd);
        }
        Err(anyhow::anyhow!("Invalid Etherscan response"))
    }

    async fn fetch_arbiscan(&self) -> Result<f64, anyhow::Error> {
        let url = "https://api.arbiscan.io/api?module=gastracker&action=gasoracle";
        let resp: Value = self.client.get(url).send().await?.json().await?;
        
        if let Some(price_str) = resp.get("result").and_then(|r| r.get("ProposeGasPrice")).and_then(|p| p.as_str()) {
            let gwei: f64 = price_str.parse()?;
            let fee_usd = 1_000_000.0 * gwei * 1e-9 * 3000.0; // L2 gas limit is higher, but gwei is tiny
            return Ok(fee_usd);
        }
        Err(anyhow::anyhow!("Invalid Arbiscan response"))
    }

    fn fallback_fee(chain: Chain) -> f64 {
        match chain {
            Chain::Ethereum => 15.00,
            Chain::Arbitrum => 0.50,
            Chain::Solana => 0.05,
            Chain::Stellar => 0.01,
        }
    }
}

impl Default for GasOracle {
    fn default() -> Self {
        Self::new()
    }
}

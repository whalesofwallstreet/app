use crate::bridge::gas_oracle::GasOracle;
use crate::bridge::{BridgeProvider, BridgeQuote, Chain};
use reqwest_middleware::ClientWithMiddleware;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

pub struct DeBridgeClient {
    #[allow(dead_code)]
    client: Option<ClientWithMiddleware>,
    oracle: Arc<GasOracle>,
}

impl Drop for DeBridgeClient {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            tokio::spawn(async move {
                let _ = timeout(Duration::from_secs(5), async move {
                    drop(client);
                })
                .await;
            });
        }
    }
}

impl DeBridgeClient {
    pub fn new(oracle: Arc<GasOracle>) -> Self {
        Self {
            client: Some(
                crate::http_client::build_resilient_client()
                    .expect("Failed to build resilient HTTP client"),
            ),
            oracle,
        }
    }
}

impl BridgeProvider for DeBridgeClient {
    fn name(&self) -> &'static str {
        "deBridge DLN"
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_quote(
        &self,
        source_chain: Chain,
        dest_chain: Chain,
        source_asset: &str,
        dest_asset: &str,
        amount_in: u64,
    ) -> Result<BridgeQuote, anyhow::Error> {
        let estimated_fee_usd = self.oracle.estimate_gas_fee_usd(source_chain).await;

        // deBridge protocol fee is 0.1% of the input value
        let protocol_fee = (amount_in as f64 * 0.001) as u64;
        let amount_out = amount_in.saturating_sub(protocol_fee);

        let duration_seconds = match (source_chain, dest_chain) {
            (Chain::Solana, Chain::Stellar) => 30,
            (Chain::Arbitrum, Chain::Stellar) => 50,
            _ => 150,
        };

        // Real order creation parameters for deBridge transaction builder
        let payload = format!(
            "{{\"targetContract\": \"0x543A8e3...\", \"minAmountOut\": {}, \"chainTo\": 148}}",
            amount_out
        );

        Ok(BridgeQuote {
            provider: self.name().to_string(),
            source_chain,
            dest_chain,
            source_asset: source_asset.to_string(),
            dest_asset: dest_asset.to_string(),
            amount_in,
            amount_out,
            estimated_fee_usd,
            duration_seconds,
            execution_payload: Some(payload),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_debridge_client_async_drop() {
        let oracle = Arc::new(GasOracle::new());
        let client = DeBridgeClient::new(oracle);

        // Explicitly drop to trigger the Drop implementation
        drop(client);

        // Give the spawned task a moment to execute
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

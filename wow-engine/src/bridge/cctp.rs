use crate::bridge::{BridgeProvider, BridgeQuote, Chain};
use reqwest_middleware::ClientWithMiddleware;

pub struct CctpClient {
    #[allow(dead_code)]
    client: ClientWithMiddleware,
}

impl CctpClient {
    pub fn new() -> Self {
        Self {
            client: crate::http_client::build_resilient_client().expect("Failed to build resilient HTTP client"),
        }
    }
}

impl Default for CctpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BridgeProvider for CctpClient {
    fn name(&self) -> &'static str {
        "Circle CCTP"
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
        // Circle CCTP has no dynamic slippage, but requires source chain gas to trigger 'depositForBurn'
        let estimated_fee_usd = match source_chain {
            Chain::Ethereum => 15.00,
            Chain::Arbitrum => 0.50,
            Chain::Solana => 0.01,
            Chain::Stellar => 0.005,
        };

        // Circle CCTP burns 1:1, meaning amount_out is equal to amount_in (minus zero protocol fee)
        let amount_out = amount_in;

        // Circle CCTP takes 15-20 minutes on Ethereum due to block finality, but is faster on Arbitrum/Solana
        let duration_seconds = match source_chain {
            Chain::Arbitrum => 180,
            Chain::Solana => 60,
            _ => 900, // 15 mins for Ethereum mainnet L1 finality
        };

        let payload = format!(
            "{{\"action\": \"depositForBurn\", \"amount\": {}, \"destinationDomain\": 3, \"mintRecipient\": \"0x8a92...\"}}",
            amount_in
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

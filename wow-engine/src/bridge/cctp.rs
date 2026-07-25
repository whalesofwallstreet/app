use crate::bridge::attestation::{
    cctp_domain, AttestationError, AttestationVerifier, CctpMessage, FileNonceStore, RpcKeySource,
};
use crate::bridge::gas_oracle::GasOracle;
use crate::bridge::{BridgeProvider, BridgeQuote, Chain};
use crate::config::AppConfig;
use reqwest_middleware::ClientWithMiddleware;
use std::sync::Arc;

pub struct CctpClient {
    #[allow(dead_code)]
    client: ClientWithMiddleware,
    oracle: Arc<GasOracle>,
    verifier: AttestationVerifier<RpcKeySource>,
}

impl CctpClient {
    pub fn new(oracle: Arc<GasOracle>) -> Self {
        let config = AppConfig::load().unwrap_or_default();
        let client = crate::http_client::build_resilient_client()
            .expect("Failed to build resilient HTTP client");
        let key_source = RpcKeySource::new(
            crate::http_client::build_resilient_client()
                .expect("Failed to build resilient HTTP client"),
            config.eth_rpc_url,
            config.cctp_message_transmitter,
        );
        let nonce_store = FileNonceStore::open(&config.cctp_nonce_store_path)
            .expect("Failed to open durable CCTP nonce store");
        Self {
            client,
            oracle,
            verifier: AttestationVerifier::new(key_source, Box::new(nonce_store)),
        }
    }

    /// Cryptographically verifies a CCTP attestation locally instead of
    /// trusting Circle's centralized attestation API. The expected
    /// destination domain is derived from the destination chain of this
    /// request. The mint transaction must not be submitted unless this
    /// returns `Ok`; the `/api/v1/cctp/verify-attestation` endpoint is the
    /// gate executors call before proceeding with a bridge leg quoted by
    /// [`BridgeProvider::get_quote`].
    pub async fn verify_attestation(
        &self,
        dest_chain: Chain,
        message: &[u8],
        attestation: &[u8],
    ) -> Result<CctpMessage, AttestationError> {
        self.verifier
            .verify(message, attestation, cctp_domain(dest_chain))
            .await
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
        let estimated_fee_usd = self.oracle.estimate_gas_fee_usd(source_chain).await;

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

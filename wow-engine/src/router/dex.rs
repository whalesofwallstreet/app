use crate::bridge::Chain;
use crate::router::slippage::{self, PoolReserves};

#[derive(Debug, Clone)]
pub struct DexQuote {
    pub provider: String,
    pub chain: Chain,
    pub source_asset: String,
    pub dest_asset: String,
    pub amount_in: u64,
    pub amount_out: u64,
    pub estimated_fee_usd: f64,
    pub duration_seconds: u64,
    /// Exact constant-product price impact of this swap, in basis points.
    pub price_impact_bps: u32,
    /// Dynamic slippage tolerance derived from the price impact.
    pub slippage_bps: u32,
}

pub struct DexProvider;

impl DexProvider {
    pub fn get_swap_quote(
        chain: Chain,
        source_asset: &str,
        dest_asset: &str,
        amount_in: u64,
    ) -> Result<DexQuote, anyhow::Error> {
        let provider_name = match chain {
            Chain::Ethereum => "Uniswap",
            Chain::Solana => "Raydium",
            Chain::Arbitrum => "Camelot",
            Chain::Stellar => "Stellar DEX",
        };

        // Mock price oracle
        let get_price = |asset: &str| -> f64 {
            match asset.to_uppercase().as_str() {
                "ETH" => 3000.0,
                "SOL" => 150.0,
                "XLM" => 0.10,
                "USDC" => 1.0,
                _ => 1.0,
            }
        };

        let price_in = get_price(source_asset);
        let price_out = get_price(dest_asset);

        // Derive constant-product reserves from the USD depth of the venue's
        // deepest pool for this pair, expressed in each pool asset.
        let depth_usd = slippage::pool_depth_usd(chain, source_asset, dest_asset);
        let reserves = PoolReserves {
            reserve_in: depth_usd / price_in,
            reserve_out: depth_usd / price_out,
        };

        // Simulate the swap on the x*y=k curve. Trades whose price impact
        // exceeds the catastrophic threshold are rejected here, before any
        // transaction payload is generated.
        let estimate =
            slippage::estimate_swap(amount_in as f64, reserves).map_err(anyhow::Error::new)?;

        let value_usd = (amount_in as f64) * price_in;

        // The 0.3% LP fee plus the value lost to price impact.
        let fee_usd = value_usd * (slippage::LP_FEE_BPS as f64) / 10_000.0;

        Ok(DexQuote {
            provider: provider_name.to_string(),
            chain,
            source_asset: source_asset.to_string(),
            dest_asset: dest_asset.to_string(),
            amount_in,
            amount_out: estimate.amount_out as u64,
            estimated_fee_usd: fee_usd,
            duration_seconds: 5,
            price_impact_bps: estimate.price_impact_bps,
            slippage_bps: estimate.recommended_slippage_bps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::slippage::MAX_PRICE_IMPACT_BPS;

    #[test]
    fn test_small_trade_gets_tight_slippage() {
        // $1k of USDC against Ethereum's $50M depth: negligible impact.
        let quote = DexProvider::get_swap_quote(Chain::Ethereum, "USDC", "ETH", 1_000).unwrap();
        assert!(quote.price_impact_bps <= 1);
        assert!(quote.slippage_bps < 50);
    }

    #[test]
    fn test_large_trade_on_shallow_chain_gets_wider_slippage() {
        // The same USD size gets a much wider tolerance on Stellar's $2M
        // depth than on Ethereum's $50M depth.
        let deep = DexProvider::get_swap_quote(Chain::Ethereum, "USDC", "XLM", 100_000).unwrap();
        let shallow = DexProvider::get_swap_quote(Chain::Stellar, "USDC", "XLM", 100_000).unwrap();
        assert!(shallow.slippage_bps > deep.slippage_bps);
        assert!(shallow.price_impact_bps > 400);
    }

    #[test]
    fn test_catastrophic_trade_is_rejected() {
        // $1M of USDC into a $2M-deep Stellar pool is ~33% price impact.
        let err =
            DexProvider::get_swap_quote(Chain::Stellar, "USDC", "XLM", 1_000_000).unwrap_err();
        let slippage_err = err
            .downcast_ref::<crate::router::slippage::SlippageError>()
            .expect("error should carry the typed slippage rejection");
        assert!(matches!(
            slippage_err,
            crate::router::slippage::SlippageError::ExcessivePriceImpact { impact_bps }
                if *impact_bps > MAX_PRICE_IMPACT_BPS
        ));
    }
}

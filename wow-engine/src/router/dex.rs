use crate::bridge::price_oracle::PriceOracle;
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
    /// For a split trade this is the average impact across tranches.
    pub price_impact_bps: u32,
    /// Dynamic slippage tolerance derived from the price impact.
    pub slippage_bps: u32,
    /// True if this trade was large enough to be automatically split into
    /// multiple tranches to reduce its blended price impact.
    pub is_split: bool,
    /// Number of tranches the trade was executed as. `1` for a normal,
    /// unsplit swap.
    pub tranches: u32,
}

/// Trades whose single-shot price impact would exceed this get automatically
/// split into several tranches against the same pool to reduce the blended
/// cost, instead of being quoted at one punishing price. This sits well
/// below [`slippage::MAX_PRICE_IMPACT_BPS`] on purpose: splitting improves
/// pricing for large trades, it does not rescue trades the catastrophic-
/// impact ceiling deliberately rejects.
pub const ORDER_SPLIT_THRESHOLD_BPS: u32 = 500;

/// Tranche counts tried, in increasing order, when a trade needs splitting.
/// Stops at the first one whose blended impact drops back under
/// [`ORDER_SPLIT_THRESHOLD_BPS`]; falls back to the largest candidate
/// otherwise (still strictly better than not splitting at all).
const SPLIT_TRANCHE_CANDIDATES: [u32; 3] = [4, 8, 16];

pub struct DexProvider;

impl DexProvider {
    pub async fn get_swap_quote(
        price_oracle: &PriceOracle,
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

        let price_in = price_oracle.get_price_usd(source_asset).await;
        let price_out = price_oracle.get_price_usd(dest_asset).await;

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

        // Large-but-not-catastrophic trades get automatically split into
        // tranches against the same pool: each smaller leg has a smaller
        // individual price impact, so the blended output is strictly better
        // than quoting the whole amount at once.
        let (amount_out, price_impact_bps, slippage_bps, tranches) =
            if estimate.price_impact_bps > ORDER_SPLIT_THRESHOLD_BPS {
                let split = Self::best_split(amount_in as f64, reserves)?;
                (
                    split.amount_out,
                    split.price_impact_bps,
                    split.recommended_slippage_bps,
                    split.tranches,
                )
            } else {
                (
                    estimate.amount_out,
                    estimate.price_impact_bps,
                    estimate.recommended_slippage_bps,
                    1,
                )
            };

        // Full execution cost of the leg: USD value in minus the USD value
        // of the simulated AMM output. This covers both the LP fee and the
        // value lost to price impact, so it reconciles exactly with
        // amount_out.
        let value_in_usd = (amount_in as f64) * price_in;
        let fee_usd = value_in_usd - amount_out * price_out;

        Ok(DexQuote {
            provider: provider_name.to_string(),
            chain,
            source_asset: source_asset.to_string(),
            dest_asset: dest_asset.to_string(),
            amount_in,
            amount_out: amount_out as u64,
            estimated_fee_usd: fee_usd,
            duration_seconds: 5,
            price_impact_bps,
            slippage_bps,
            is_split: tranches > 1,
            tranches,
        })
    }

    /// Tries increasing tranche counts and returns the first whose blended
    /// price impact drops back under [`ORDER_SPLIT_THRESHOLD_BPS`], or the
    /// result of the largest candidate tried if none get there.
    fn best_split(
        amount_in: f64,
        reserves: PoolReserves,
    ) -> Result<slippage::SplitSwapEstimate, anyhow::Error> {
        let mut best: Option<slippage::SplitSwapEstimate> = None;
        for &tranches in &SPLIT_TRANCHE_CANDIDATES {
            let split = slippage::estimate_split_swap(amount_in, reserves, tranches)
                .map_err(anyhow::Error::new)?;
            let good_enough = split.price_impact_bps <= ORDER_SPLIT_THRESHOLD_BPS;
            best = Some(split);
            if good_enough {
                break;
            }
        }
        Ok(best.expect("SPLIT_TRANCHE_CANDIDATES is non-empty"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::slippage::MAX_PRICE_IMPACT_BPS;

    /// Every test below cares about deterministic pool depth/slippage math,
    /// not live pricing, so it uses the network-free fallback table — the
    /// same constants this file hardcoded before `PriceOracle` existed.
    fn oracle() -> PriceOracle {
        PriceOracle::new_for_tests()
    }

    #[tokio::test]
    async fn test_small_trade_gets_tight_slippage() {
        // $1k of USDC against Ethereum's $50M depth: negligible impact.
        let quote = DexProvider::get_swap_quote(&oracle(), Chain::Ethereum, "USDC", "ETH", 1_000)
            .await
            .unwrap();
        assert!(quote.price_impact_bps <= 1);
        assert!(quote.slippage_bps < 50);
    }

    #[tokio::test]
    async fn test_large_trade_on_shallow_chain_gets_wider_slippage() {
        // The same USD size gets a much wider tolerance on Stellar's $2M
        // depth than on Ethereum's $50M depth.
        let deep = DexProvider::get_swap_quote(&oracle(), Chain::Ethereum, "USDC", "XLM", 100_000)
            .await
            .unwrap();
        let shallow =
            DexProvider::get_swap_quote(&oracle(), Chain::Stellar, "USDC", "XLM", 100_000)
                .await
                .unwrap();
        assert!(shallow.slippage_bps > deep.slippage_bps);
        assert!(shallow.price_impact_bps > 400);
    }

    #[tokio::test]
    async fn test_fee_reconciles_with_amm_output() {
        // On a trade with real price impact the reported fee must equal the
        // USD value in minus the USD value out, not just the flat LP fee.
        let amount_in: u64 = 100_000; // USDC into Stellar's $2M pool: ~4.7% impact
        let quote =
            DexProvider::get_swap_quote(&oracle(), Chain::Stellar, "USDC", "XLM", amount_in)
                .await
                .unwrap();

        let value_in_usd = amount_in as f64 * 1.0;
        let value_out_usd = quote.amount_out as f64 * 0.10;
        // amount_out is truncated to whole output units, so allow up to one
        // unit of drift in USD terms.
        assert!((quote.estimated_fee_usd - (value_in_usd - value_out_usd)).abs() <= 0.10);
        // The reconciled fee must exceed the flat LP fee alone, since this
        // trade has significant price impact on top of it.
        assert!(quote.estimated_fee_usd > value_in_usd * 0.003);
    }

    #[tokio::test]
    async fn test_large_trade_is_automatically_split() {
        // $200k of USDC into Stellar's $2M pool is a ~907 bps lump impact:
        // well above the 500 bps split threshold, but far short of the
        // 1500 bps catastrophic ceiling.
        let lump_impact_only = {
            let reserves = crate::router::slippage::PoolReserves {
                reserve_in: 2_000_000.0,
                reserve_out: 2_000_000.0,
            };
            crate::router::slippage::estimate_swap(200_000.0, reserves).unwrap()
        };
        assert!(lump_impact_only.price_impact_bps > ORDER_SPLIT_THRESHOLD_BPS);

        let quote = DexProvider::get_swap_quote(&oracle(), Chain::Stellar, "USDC", "XLM", 200_000)
            .await
            .unwrap();

        assert!(quote.is_split, "large trade should be split");
        assert!(quote.tranches > 1);
        assert!(
            quote.price_impact_bps < lump_impact_only.price_impact_bps,
            "splitting should improve the blended price impact over the lump quote"
        );
    }

    #[tokio::test]
    async fn test_small_trade_is_not_split() {
        let quote = DexProvider::get_swap_quote(&oracle(), Chain::Ethereum, "USDC", "ETH", 1_000)
            .await
            .unwrap();
        assert!(!quote.is_split);
        assert_eq!(quote.tranches, 1);
    }

    #[tokio::test]
    async fn test_catastrophic_trade_is_still_rejected_not_rescued_by_splitting() {
        // Splitting improves pricing for large trades; it must not silently
        // bypass the hard catastrophic-impact safety ceiling.
        let err = DexProvider::get_swap_quote(&oracle(), Chain::Stellar, "USDC", "XLM", 1_000_000)
            .await
            .unwrap_err();
        assert!(err
            .downcast_ref::<crate::router::slippage::SlippageError>()
            .is_some());
    }

    #[tokio::test]
    async fn test_catastrophic_trade_is_rejected() {
        // $1M of USDC into a $2M-deep Stellar pool is ~33% price impact.
        let err = DexProvider::get_swap_quote(&oracle(), Chain::Stellar, "USDC", "XLM", 1_000_000)
            .await
            .unwrap_err();
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

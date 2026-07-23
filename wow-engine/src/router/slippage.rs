//! Dynamic slippage estimation from AMM pool depth.
//!
//! Instead of applying one static slippage value to every route, this module
//! computes the exact constant-product (x*y=k) price impact of a trade from
//! the pool reserves and the input size, and derives a per-leg slippage
//! tolerance from it. Trades whose price impact exceeds a catastrophic
//! threshold are rejected outright instead of being sent on-chain to fail.

use crate::bridge::Chain;
use thiserror::Error;

/// Uniswap V2-style liquidity provider fee, in basis points.
pub const LP_FEE_BPS: u32 = 30;

/// Buffer added on top of the computed price impact to absorb ordinary
/// inter-block price movement, in basis points.
pub const VOLATILITY_BUFFER_BPS: u32 = 20;

/// Hard ceiling on acceptable price impact. Trades above this are rejected
/// instead of quoted: executing them would be catastrophic for the user.
pub const MAX_PRICE_IMPACT_BPS: u32 = 1500;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum SlippageError {
    #[error(
        "price impact of {impact_bps} bps ({percent:.2}%) exceeds the maximum of {MAX_PRICE_IMPACT_BPS} bps; \
         reduce the trade size or use a deeper pool",
        percent = *impact_bps as f64 / 100.0
    )]
    ExcessivePriceImpact { impact_bps: u32 },

    #[error("pool has no liquidity for this pair")]
    NoLiquidity,

    #[error("trade amount must be greater than zero")]
    ZeroAmount,
}

/// Reserves of a constant-product pool, denominated in the two pool assets.
#[derive(Debug, Clone, Copy)]
pub struct PoolReserves {
    pub reserve_in: f64,
    pub reserve_out: f64,
}

/// Result of simulating a trade against a pool.
#[derive(Debug, Clone, Copy)]
pub struct SlippageEstimate {
    /// Exact constant-product price impact of this trade, in basis points.
    pub price_impact_bps: u32,
    /// Slippage tolerance to use for the transaction: price impact plus a
    /// volatility buffer. This is what `minAmountOut` should be derived from.
    pub recommended_slippage_bps: u32,
    /// Simulated output amount after the LP fee and price impact.
    pub amount_out: f64,
}

/// Simulates `amount_in` against a constant-product pool and derives the
/// dynamic slippage tolerance for the trade.
///
/// Uses the Uniswap V2 swap curve with the LP fee applied on input:
///
/// ```text
/// dx' = dx * (1 - fee)
/// dy  = (dx' * y) / (x + dx')
/// ```
///
/// Price impact is the deviation of the execution price from the spot price
/// `y / x`, which for the curve above reduces to `dx' / (x + dx')`.
pub fn estimate_swap(
    amount_in: f64,
    reserves: PoolReserves,
) -> Result<SlippageEstimate, SlippageError> {
    if amount_in <= 0.0 {
        return Err(SlippageError::ZeroAmount);
    }
    if reserves.reserve_in <= 0.0 || reserves.reserve_out <= 0.0 {
        return Err(SlippageError::NoLiquidity);
    }

    let fee_multiplier = 1.0 - (LP_FEE_BPS as f64) / 10_000.0;
    let amount_in_after_fee = amount_in * fee_multiplier;

    let amount_out =
        (amount_in_after_fee * reserves.reserve_out) / (reserves.reserve_in + amount_in_after_fee);

    let price_impact = amount_in_after_fee / (reserves.reserve_in + amount_in_after_fee);
    let price_impact_bps = (price_impact * 10_000.0).round() as u32;

    if price_impact_bps > MAX_PRICE_IMPACT_BPS {
        return Err(SlippageError::ExcessivePriceImpact {
            impact_bps: price_impact_bps,
        });
    }

    Ok(SlippageEstimate {
        price_impact_bps,
        recommended_slippage_bps: price_impact_bps + VOLATILITY_BUFFER_BPS,
        amount_out,
    })
}

/// USD depth of one side of the deepest pool for a pair on a chain.
///
/// Stands in for live reserve queries against DEX APIs, in the same spirit as
/// the engine's mock price oracle: values approximate the relative liquidity
/// of the major venue on each chain so that estimates scale realistically
/// with trade size.
pub fn pool_depth_usd(chain: Chain, asset_a: &str, asset_b: &str) -> f64 {
    let chain_depth = match chain {
        Chain::Ethereum => 50_000_000.0,
        Chain::Arbitrum => 10_000_000.0,
        Chain::Solana => 20_000_000.0,
        Chain::Stellar => 2_000_000.0,
    };

    // Pairs that include a major quote asset trade in the deepest pools;
    // exotic pairs route through shallower ones.
    let has_major = [asset_a, asset_b]
        .iter()
        .any(|asset| matches!(asset.to_uppercase().as_str(), "USDC" | "ETH" | "SOL"));
    if has_major {
        chain_depth
    } else {
        chain_depth / 10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-9;

    #[test]
    fn test_amount_out_matches_uniswap_v2_curve() {
        // Canonical Uniswap V2 vector: x = 1000, y = 1000, dx = 100, 0.3% fee.
        // dy = (100 * 0.997 * 1000) / (1000 + 100 * 0.997) = 90.66108938801491
        let estimate = estimate_swap(
            100.0,
            PoolReserves {
                reserve_in: 1000.0,
                reserve_out: 1000.0,
            },
        )
        .unwrap();

        assert!((estimate.amount_out - 90.661_089_388_014_91).abs() < EPSILON);

        // Price impact: 99.7 / 1099.7 = 9.0661% -> 907 bps rounded.
        assert_eq!(estimate.price_impact_bps, 907);
        assert_eq!(
            estimate.recommended_slippage_bps,
            907 + VOLATILITY_BUFFER_BPS
        );
    }

    #[test]
    fn test_constant_product_invariant_holds() {
        // After the swap, x' * y' must equal k computed on fee-adjusted input.
        let reserves = PoolReserves {
            reserve_in: 5_000.0,
            reserve_out: 20_000.0,
        };
        let amount_in = 250.0;
        let estimate = estimate_swap(amount_in, reserves).unwrap();

        let dx_after_fee = amount_in * 0.997;
        let k_before = reserves.reserve_in * reserves.reserve_out;
        let k_after =
            (reserves.reserve_in + dx_after_fee) * (reserves.reserve_out - estimate.amount_out);
        assert!((k_before - k_after).abs() / k_before < 1e-12);
    }

    #[test]
    fn test_slippage_scales_with_trade_size() {
        let reserves = PoolReserves {
            reserve_in: 1_000_000.0,
            reserve_out: 1_000_000.0,
        };

        let small = estimate_swap(1_000.0, reserves).unwrap();
        let medium = estimate_swap(10_000.0, reserves).unwrap();
        let large = estimate_swap(100_000.0, reserves).unwrap();

        // A $1k trade against $1M depth is ~10 bps; a $100k trade is ~907 bps.
        assert!(small.price_impact_bps < medium.price_impact_bps);
        assert!(medium.price_impact_bps < large.price_impact_bps);
        assert_eq!(small.price_impact_bps, 10);
        assert_eq!(large.price_impact_bps, 907);
    }

    #[test]
    fn test_slippage_scales_with_pool_depth() {
        let deep = estimate_swap(
            10_000.0,
            PoolReserves {
                reserve_in: 50_000_000.0,
                reserve_out: 50_000_000.0,
            },
        )
        .unwrap();
        let shallow = estimate_swap(
            10_000.0,
            PoolReserves {
                reserve_in: 100_000.0,
                reserve_out: 100_000.0,
            },
        )
        .unwrap();

        assert!(deep.price_impact_bps < shallow.price_impact_bps);
    }

    #[test]
    fn test_catastrophic_price_impact_is_rejected() {
        // dx' / (x + dx') > 15% requires dx > ~0.177 x; use 0.25 x.
        let err = estimate_swap(
            250_000.0,
            PoolReserves {
                reserve_in: 1_000_000.0,
                reserve_out: 1_000_000.0,
            },
        )
        .unwrap_err();

        match err {
            SlippageError::ExcessivePriceImpact { impact_bps } => {
                assert!(impact_bps > MAX_PRICE_IMPACT_BPS);
            }
            other => panic!("expected ExcessivePriceImpact, got {other:?}"),
        }
    }

    #[test]
    fn test_impact_just_below_threshold_is_accepted() {
        // dx = 0.17 x -> dx' / (x + dx') = 0.16949 / 1.16949 = 14.49% < 15%.
        let estimate = estimate_swap(
            170_000.0,
            PoolReserves {
                reserve_in: 1_000_000.0,
                reserve_out: 1_000_000.0,
            },
        )
        .unwrap();
        assert!(estimate.price_impact_bps <= MAX_PRICE_IMPACT_BPS);
        assert_eq!(estimate.price_impact_bps, 1449);
    }

    #[test]
    fn test_invalid_inputs_are_rejected() {
        let reserves = PoolReserves {
            reserve_in: 1_000.0,
            reserve_out: 1_000.0,
        };
        assert_eq!(
            estimate_swap(0.0, reserves).unwrap_err(),
            SlippageError::ZeroAmount
        );
        assert_eq!(
            estimate_swap(
                100.0,
                PoolReserves {
                    reserve_in: 0.0,
                    reserve_out: 1_000.0
                }
            )
            .unwrap_err(),
            SlippageError::NoLiquidity
        );
    }

    #[test]
    fn test_pool_depth_ranking() {
        // Major pairs on Ethereum are the deepest; exotic pairs on Stellar
        // are the shallowest.
        let eth_major = pool_depth_usd(Chain::Ethereum, "ETH", "USDC");
        let stellar_major = pool_depth_usd(Chain::Stellar, "XLM", "USDC");
        let stellar_exotic = pool_depth_usd(Chain::Stellar, "XLM", "EURT");
        assert!(eth_major > stellar_major);
        assert!(stellar_major > stellar_exotic);
    }
}

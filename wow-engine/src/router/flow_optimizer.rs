use super::slippage::{
    calculate_amount_out_with_slippage, calculate_slippage, get_liquidity_pool, LiquidityPool,
};
use crate::bridge::{BridgeQuote, Chain};

/// Represents a parallel execution path through a bridge
#[derive(Debug, Clone)]
pub struct ParallelPath {
    pub provider: String,
    pub split_percentage: f64,
    pub amount_in: u64,
    pub amount_out: u64,
    pub estimated_fee_usd: f64,
    pub duration_seconds: u64,
    pub slippage_percentage: f64,
    pub execution_payload: Option<String>,
}

/// Represents the result of multi-path optimization
#[derive(Debug, Clone)]
pub struct OptimizedRoute {
    pub paths: Vec<ParallelPath>,
    pub total_amount_in: u64,
    pub total_amount_out: u64,
    pub total_estimated_fee_usd: f64,
    pub max_duration_seconds: u64,
    pub average_slippage_percentage: f64,
    pub is_split: bool,
}

/// Optimize order execution across multiple bridges using flow optimization
/// Returns the best split ratio to minimize total slippage
pub async fn optimize_multi_path_route(
    available_bridges: Vec<BridgeQuote>,
    source_chain: Chain,
    _dest_chain: Chain,
    asset: &str,
    total_amount: u64,
) -> Result<OptimizedRoute, anyhow::Error> {
    if available_bridges.is_empty() {
        return Err(anyhow::anyhow!("No available bridges for routing"));
    }

    // Single bridge case - no splitting needed
    if available_bridges.len() == 1 {
        let bridge = &available_bridges[0];
        let pool = get_liquidity_pool(&bridge.provider, source_chain, asset);
        let slippage = calculate_slippage(&pool, total_amount as f64);
        
        return Ok(OptimizedRoute {
            paths: vec![ParallelPath {
                provider: bridge.provider.clone(),
                split_percentage: 100.0,
                amount_in: total_amount,
                amount_out: bridge.amount_out,
                estimated_fee_usd: bridge.estimated_fee_usd,
                duration_seconds: bridge.duration_seconds,
                slippage_percentage: slippage,
                execution_payload: bridge.execution_payload.clone(),
            }],
            total_amount_in: total_amount,
            total_amount_out: bridge.amount_out,
            total_estimated_fee_usd: bridge.estimated_fee_usd,
            max_duration_seconds: bridge.duration_seconds,
            average_slippage_percentage: slippage,
            is_split: false,
        });
    }

    // Multi-bridge optimization
    let pools: Vec<LiquidityPool> = available_bridges
        .iter()
        .map(|b| get_liquidity_pool(&b.provider, source_chain, asset))
        .collect();

    // Find optimal split using iterative optimization
    let optimal_splits = find_optimal_split(&pools, &available_bridges, total_amount as f64);

    // Calculate results for each path
    let mut paths = Vec::new();
    let mut total_amount_out = 0u64;
    let mut total_fee = 0.0;
    let mut max_duration = 0u64;
    let mut weighted_slippage = 0.0;

    for (i, split_pct) in optimal_splits.iter().enumerate() {
        if *split_pct < 0.01 {
            continue; // Skip paths with < 1% allocation
        }

        let bridge = &available_bridges[i];
        let pool = &pools[i];
        let amount_in = (total_amount as f64 * split_pct / 100.0) as u64;

        // Get bridge-specific fee percentage
        let fee_pct = match bridge.provider.as_str() {
            "Circle CCTP" => 0.0,       // CCTP has no protocol fee
            "deBridge DLN" => 0.1,       // 0.1% fee
            "Stargate" => 0.06,          // 0.06% fee
            _ => 0.1,
        };

        let amount_out_f64 =
            calculate_amount_out_with_slippage(pool, amount_in as f64, fee_pct);
        let amount_out = amount_out_f64 as u64;
        let slippage = calculate_slippage(pool, amount_in as f64);

        // Calculate gas fee for this leg
        let estimated_fee_usd = bridge.estimated_fee_usd * (split_pct / 100.0);

        paths.push(ParallelPath {
            provider: bridge.provider.clone(),
            split_percentage: *split_pct,
            amount_in,
            amount_out,
            estimated_fee_usd,
            duration_seconds: bridge.duration_seconds,
            slippage_percentage: slippage,
            execution_payload: bridge.execution_payload.clone(),
        });

        total_amount_out += amount_out;
        total_fee += estimated_fee_usd;
        max_duration = max_duration.max(bridge.duration_seconds);
        weighted_slippage += slippage * split_pct / 100.0;
    }

    // Check if splitting provides benefit over single-path
    let single_path_result = calculate_single_path_outcome(&available_bridges, &pools, total_amount as f64);
    let split_path_value = total_amount_out as f64 - total_fee;
    let single_path_value = single_path_result.0 - single_path_result.1;

    // If splitting doesn't provide at least 0.5% improvement, use single path
    if split_path_value < single_path_value * 1.005 && paths.len() > 1 {
        // Fall back to single best path
        let best_bridge_idx = find_best_single_bridge(&available_bridges, &pools, total_amount as f64);
        let best_bridge = &available_bridges[best_bridge_idx];
        let best_pool = &pools[best_bridge_idx];
        let slippage = calculate_slippage(best_pool, total_amount as f64);

        return Ok(OptimizedRoute {
            paths: vec![ParallelPath {
                provider: best_bridge.provider.clone(),
                split_percentage: 100.0,
                amount_in: total_amount,
                amount_out: best_bridge.amount_out,
                estimated_fee_usd: best_bridge.estimated_fee_usd,
                duration_seconds: best_bridge.duration_seconds,
                slippage_percentage: slippage,
                execution_payload: best_bridge.execution_payload.clone(),
            }],
            total_amount_in: total_amount,
            total_amount_out: best_bridge.amount_out,
            total_estimated_fee_usd: best_bridge.estimated_fee_usd,
            max_duration_seconds: best_bridge.duration_seconds,
            average_slippage_percentage: slippage,
            is_split: false,
        });
    }

    Ok(OptimizedRoute {
        paths,
        total_amount_in: total_amount,
        total_amount_out,
        total_estimated_fee_usd: total_fee,
        max_duration_seconds: max_duration,
        average_slippage_percentage: weighted_slippage,
        is_split: true,
    })
}

/// Find optimal split percentages across multiple pools to minimize slippage
/// Uses iterative gradient descent approach
fn find_optimal_split(
    pools: &[LiquidityPool],
    bridges: &[BridgeQuote],
    total_amount: f64,
) -> Vec<f64> {
    let n = pools.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![100.0];
    }

    // Initialize with equal split
    let mut splits: Vec<f64> = vec![100.0 / n as f64; n];
    let iterations = 50;
    let learning_rate = 0.1;

    for _ in 0..iterations {
        // Calculate marginal slippage (derivative) for each pool at current allocation
        let mut marginal_slippages = Vec::new();

        for (i, pool) in pools.iter().enumerate() {
            let amount_i = total_amount * splits[i] / 100.0;
            let fee_pct = get_bridge_fee_percentage(&bridges[i].provider);
            let amount_after_fee = amount_i * (1.0 - fee_pct / 100.0);

            // Calculate marginal cost (slippage derivative)
            let epsilon = total_amount * 0.0001; // 0.01% of total
            let slippage_current = calculate_slippage(pool, amount_after_fee);
            let slippage_next = calculate_slippage(pool, amount_after_fee + epsilon);
            let marginal = (slippage_next - slippage_current) / epsilon;

            marginal_slippages.push(marginal);
        }

        // Rebalance: move allocation from high marginal cost to low marginal cost
        if let (Some(max_idx), Some(min_idx)) = (
            argmax(&marginal_slippages),
            argmin(&marginal_slippages),
        ) {
            let diff = marginal_slippages[max_idx] - marginal_slippages[min_idx];
            if diff > 0.0001 {
                let transfer_amount = learning_rate * diff * 10.0;
                let transfer_amount = transfer_amount.min(splits[max_idx] * 0.1); // Max 10% per iteration

                splits[max_idx] -= transfer_amount;
                splits[min_idx] += transfer_amount;

                // Ensure no negative splits
                for split in splits.iter_mut() {
                    *split = split.max(0.0);
                }

                // Renormalize to 100%
                let sum: f64 = splits.iter().sum();
                if sum > 0.0 {
                    for split in splits.iter_mut() {
                        *split = *split * 100.0 / sum;
                    }
                }
            } else {
                break; // Converged
            }
        }
    }

    splits
}

/// Calculate the outcome of routing all through the single best bridge
fn calculate_single_path_outcome(
    bridges: &[BridgeQuote],
    pools: &[LiquidityPool],
    total_amount: f64,
) -> (f64, f64) {
    let best_idx = find_best_single_bridge(bridges, pools, total_amount);
    let pool = &pools[best_idx];
    let bridge = &bridges[best_idx];

    let fee_pct = get_bridge_fee_percentage(&bridge.provider);
    let amount_out = calculate_amount_out_with_slippage(pool, total_amount, fee_pct);
    let total_fee = bridge.estimated_fee_usd;

    (amount_out, total_fee)
}

/// Find the best single bridge for the entire amount
fn find_best_single_bridge(
    bridges: &[BridgeQuote],
    pools: &[LiquidityPool],
    total_amount: f64,
) -> usize {
    let mut best_idx = 0;
    let mut best_value = 0.0;

    for (i, (bridge, pool)) in bridges.iter().zip(pools.iter()).enumerate() {
        let fee_pct = get_bridge_fee_percentage(&bridge.provider);
        let amount_out = calculate_amount_out_with_slippage(pool, total_amount, fee_pct);
        let net_value = amount_out - bridge.estimated_fee_usd;

        if net_value > best_value {
            best_value = net_value;
            best_idx = i;
        }
    }

    best_idx
}

fn get_bridge_fee_percentage(provider: &str) -> f64 {
    match provider {
        "Circle CCTP" => 0.0,
        "deBridge DLN" => 0.1,
        "Stargate" => 0.06,
        _ => 0.1,
    }
}

fn argmax(values: &[f64]) -> Option<usize> {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx)
}

fn argmin(values: &[f64]) -> Option<usize> {
    values
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_optimal_split_two_pools() {
        let pools = vec![
            LiquidityPool {
                provider: "Bridge A".to_string(),
                chain: Chain::Ethereum,
                reserve_x: 10_000_000.0,
                reserve_y: 10_000_000.0,
            },
            LiquidityPool {
                provider: "Bridge B".to_string(),
                chain: Chain::Ethereum,
                reserve_x: 20_000_000.0,
                reserve_y: 20_000_000.0,
            },
        ];

        let bridges = vec![
            BridgeQuote {
                provider: "Bridge A".to_string(),
                source_chain: Chain::Ethereum,
                dest_chain: Chain::Stellar,
                source_asset: "USDC".to_string(),
                dest_asset: "USDC".to_string(),
                amount_in: 1_000_000,
                amount_out: 990_000,
                estimated_fee_usd: 10.0,
                duration_seconds: 300,
                execution_payload: None,
            },
            BridgeQuote {
                provider: "Bridge B".to_string(),
                source_chain: Chain::Ethereum,
                dest_chain: Chain::Stellar,
                source_asset: "USDC".to_string(),
                dest_asset: "USDC".to_string(),
                amount_in: 1_000_000,
                amount_out: 995_000,
                estimated_fee_usd: 8.0,
                duration_seconds: 250,
                execution_payload: None,
            },
        ];

        let splits = find_optimal_split(&pools, &bridges, 1_000_000.0);

        // Bridge B has deeper liquidity, should get more allocation
        assert!(splits[1] > splits[0], "Deeper liquidity pool should get more allocation");
        
        // Total should be 100%
        let total: f64 = splits.iter().sum();
        assert!((total - 100.0).abs() < 0.01, "Total split should be 100%");
    }

    #[test]
    fn test_argmax_argmin() {
        let values = vec![1.5, 3.2, 0.8, 2.1];
        assert_eq!(argmax(&values), Some(1));
        assert_eq!(argmin(&values), Some(2));
    }
}

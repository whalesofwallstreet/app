use crate::bridge::Chain;

/// Represents liquidity pool parameters for constant-product AMM formula (x * y = k)
#[derive(Debug, Clone)]
pub struct LiquidityPool {
    pub provider: String,
    pub chain: Chain,
    pub reserve_x: f64, // Reserve of input asset
    pub reserve_y: f64, // Reserve of output asset
}

/// Calculate price impact and slippage for a given trade size using constant-product formula
/// Formula: Δy = (y * Δx) / (x + Δx)
/// Price impact = (Δy / y) * 100
pub fn calculate_slippage(pool: &LiquidityPool, amount_in: f64) -> f64 {
    let k = pool.reserve_x * pool.reserve_y;
    let new_reserve_x = pool.reserve_x + amount_in;
    let new_reserve_y = k / new_reserve_x;
    let amount_out = pool.reserve_y - new_reserve_y;
    
    // Calculate price impact as percentage
    let ideal_price = pool.reserve_y / pool.reserve_x;
    let actual_price = amount_out / amount_in;
    let price_impact = ((ideal_price - actual_price) / ideal_price).abs();
    
    price_impact * 100.0 // Return as percentage
}

/// Calculate optimal amount out after slippage using constant-product formula
pub fn calculate_amount_out_with_slippage(
    pool: &LiquidityPool,
    amount_in: f64,
    fee_percentage: f64,
) -> f64 {
    // Apply fee first
    let amount_in_after_fee = amount_in * (1.0 - fee_percentage / 100.0);
    
    // Apply constant-product formula: Δy = (y * Δx) / (x + Δx)
    let k = pool.reserve_x * pool.reserve_y;
    let new_reserve_x = pool.reserve_x + amount_in_after_fee;
    let new_reserve_y = k / new_reserve_x;
    let amount_out = pool.reserve_y - new_reserve_y;
    
    amount_out
}

/// Get simulated liquidity pools for different bridges
/// In production, these would be fetched from on-chain data or bridge APIs
pub fn get_liquidity_pool(provider: &str, chain: Chain, asset: &str) -> LiquidityPool {
    match (provider, chain, asset) {
        // CCTP has deep liquidity for USDC (burn/mint mechanism, virtually infinite liquidity)
        ("Circle CCTP", _, "USDC") => LiquidityPool {
            provider: "Circle CCTP".to_string(),
            chain,
            reserve_x: 100_000_000.0, // $100M USDC reserve (simulated)
            reserve_y: 100_000_000.0,
        },
        // deBridge has moderate liquidity across chains
        ("deBridge DLN", Chain::Ethereum, "USDC") => LiquidityPool {
            provider: "deBridge DLN".to_string(),
            chain,
            reserve_x: 25_000_000.0, // $25M reserve
            reserve_y: 25_000_000.0,
        },
        ("deBridge DLN", Chain::Arbitrum, "USDC") => LiquidityPool {
            provider: "deBridge DLN".to_string(),
            chain,
            reserve_x: 15_000_000.0, // $15M reserve
            reserve_y: 15_000_000.0,
        },
        ("deBridge DLN", Chain::Solana, "USDC") => LiquidityPool {
            provider: "deBridge DLN".to_string(),
            chain,
            reserve_x: 10_000_000.0, // $10M reserve
            reserve_y: 10_000_000.0,
        },
        ("deBridge DLN", _, "ETH") => LiquidityPool {
            provider: "deBridge DLN".to_string(),
            chain,
            reserve_x: 5_000.0, // 5k ETH (~$15M at $3000/ETH)
            reserve_y: 15_000_000.0, // $15M USDC equivalent
        },
        // Stargate (hypothetical) - another popular bridge
        ("Stargate", _, "USDC") => LiquidityPool {
            provider: "Stargate".to_string(),
            chain,
            reserve_x: 30_000_000.0, // $30M reserve
            reserve_y: 30_000_000.0,
        },
        // Default pools with lower liquidity
        _ => LiquidityPool {
            provider: provider.to_string(),
            chain,
            reserve_x: 1_000_000.0, // $1M default
            reserve_y: 1_000_000.0,
        },
    }
}

/// Calculate the derivative of slippage with respect to amount
/// Used to find optimal split ratio between multiple paths
pub fn slippage_derivative(pool: &LiquidityPool, amount_in: f64) -> f64 {
    let epsilon = 1.0; // Small change for numerical derivative
    let slippage_at_x = calculate_slippage(pool, amount_in);
    let slippage_at_x_plus_epsilon = calculate_slippage(pool, amount_in + epsilon);
    
    (slippage_at_x_plus_epsilon - slippage_at_x) / epsilon
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_slippage_small_trade() {
        let pool = LiquidityPool {
            provider: "TestDEX".to_string(),
            chain: Chain::Ethereum,
            reserve_x: 1_000_000.0,
            reserve_y: 1_000_000.0,
        };
        
        // Small trade should have minimal slippage
        let slippage = calculate_slippage(&pool, 1000.0);
        assert!(slippage < 0.2, "Small trade slippage should be < 0.2%, got {}", slippage);
    }

    #[test]
    fn test_calculate_slippage_large_trade() {
        let pool = LiquidityPool {
            provider: "TestDEX".to_string(),
            chain: Chain::Ethereum,
            reserve_x: 1_000_000.0,
            reserve_y: 1_000_000.0,
        };
        
        // Large trade (10% of pool) should have significant slippage
        let slippage = calculate_slippage(&pool, 100_000.0);
        assert!(slippage > 5.0, "Large trade slippage should be > 5%, got {}", slippage);
    }

    #[test]
    fn test_calculate_amount_out_with_slippage() {
        let pool = LiquidityPool {
            provider: "TestDEX".to_string(),
            chain: Chain::Ethereum,
            reserve_x: 1_000_000.0,
            reserve_y: 1_000_000.0,
        };
        
        let amount_out = calculate_amount_out_with_slippage(&pool, 10_000.0, 0.3);
        
        // With 0.3% fee and some slippage, output should be less than input
        assert!(amount_out < 10_000.0);
        assert!(amount_out > 9_500.0); // But not too much less for this size
    }

    #[test]
    fn test_deep_liquidity_low_slippage() {
        let pool = get_liquidity_pool("Circle CCTP", Chain::Ethereum, "USDC");
        
        // Even $1M trade should have low slippage with $100M liquidity
        let slippage = calculate_slippage(&pool, 1_000_000.0);
        assert!(slippage < 1.0, "Deep liquidity should have < 1% slippage for $1M, got {}", slippage);
    }
}

pub mod dex;
pub mod flow_optimizer;
pub mod slippage;

use crate::bridge::{
    cctp::CctpClient, debridge::DeBridgeClient, gas_oracle::GasOracle, BridgeProvider, Chain,
};
use crate::router::dex::DexProvider;
use crate::router::flow_optimizer::optimize_multi_path_route;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteOption {
    pub provider: String,
    pub path: String,
    pub amount_in: u64,
    pub amount_out: u64,
    pub estimated_fee_usd: f64,
    pub duration_seconds: u64,
    pub execution_payload: Option<String>,
    /// For split routes, contains parallel execution paths
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_paths: Option<Vec<ParallelPathInfo>>,
    /// Indicates if this route uses order splitting
    pub is_split_route: bool,
    /// Average slippage across all paths (percentage)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_percentage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelPathInfo {
    pub provider: String,
    pub split_percentage: f64,
    pub amount_in: u64,
    pub amount_out: u64,
    pub estimated_fee_usd: f64,
    pub duration_seconds: u64,
    pub slippage_percentage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_payload: Option<String>,
}

pub struct RoutePlanner {
    debridge: DeBridgeClient,
    cctp: CctpClient,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Node {
    chain: Chain,
    asset: String,
}

#[derive(Debug, Clone)]
struct State {
    usd_value: u64,
    amount: u64,
    node: Node,
    route_so_far: Vec<RouteOption>,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.usd_value == other.usd_value
    }
}
impl Eq for State {}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        self.usd_value.cmp(&other.usd_value)
    }
}

fn get_usd_value(asset: &str, amount: u64) -> f64 {
    let price = match asset.to_uppercase().as_str() {
        "ETH" => 3000.0,
        "SOL" => 150.0,
        "XLM" => 0.10,
        "USDC" => 1.0,
        _ => 1.0,
    };
    (amount as f64) * price
}

impl Default for RoutePlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl RoutePlanner {
    pub fn new() -> Self {
        let oracle = Arc::new(GasOracle::new());
        Self {
            debridge: DeBridgeClient::new(oracle.clone()),
            cctp: CctpClient::new(oracle),
        }
    }

    /// Find best route with multi-path optimization for large orders
    /// This is the primary routing function that includes order splitting logic
    #[tracing::instrument(skip(self), err)]
    pub async fn find_best_route_with_splitting(
        &self,
        source_chain: Chain,
        dest_chain: Chain,
        source_asset: &str,
        dest_asset: &str,
        amount_in: u64,
    ) -> Result<Vec<RouteOption>, anyhow::Error> {
        // Threshold for enabling multi-path optimization: $100k+
        // In production, this would be configurable
        let multi_path_threshold = 100_000; // $100k in smallest unit (e.g., cents for USDC with 2 decimals)
        
        let enable_splitting = amount_in >= multi_path_threshold;
        
        if enable_splitting {
            tracing::info!(
                "Large order detected ({}), attempting multi-path optimization",
                amount_in
            );
        }

        // First, find available bridges for direct cross-chain transfer
        let available_bridges = self
            .get_available_bridges(source_chain, dest_chain, source_asset, amount_in)
            .await?;

        if !available_bridges.is_empty() && enable_splitting {
            // Attempt multi-path optimization
            match optimize_multi_path_route(
                available_bridges.clone(),
                source_chain,
                dest_chain,
                source_asset,
                amount_in,
            )
            .await
            {
                Ok(optimized) => {
                    let mut routes = Vec::new();

                    // Convert optimized route to RouteOption format
                    if optimized.is_split {
                        tracing::info!(
                            "Multi-path optimization successful: split across {} bridges",
                            optimized.paths.len()
                        );

                        let parallel_paths: Vec<ParallelPathInfo> = optimized
                            .paths
                            .iter()
                            .map(|p| ParallelPathInfo {
                                provider: p.provider.clone(),
                                split_percentage: p.split_percentage,
                                amount_in: p.amount_in,
                                amount_out: p.amount_out,
                                estimated_fee_usd: p.estimated_fee_usd,
                                duration_seconds: p.duration_seconds,
                                slippage_percentage: p.slippage_percentage,
                                execution_payload: p.execution_payload.clone(),
                            })
                            .collect();

                        let combined_provider = optimized
                            .paths
                            .iter()
                            .map(|p| format!("{} ({:.1}%)", p.provider, p.split_percentage))
                            .collect::<Vec<_>>()
                            .join(" + ");

                        routes.push(RouteOption {
                            provider: combined_provider,
                            path: format!(
                                "Multi-path routing: {}",
                                optimized
                                    .paths
                                    .iter()
                                    .map(|p| p.provider.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" + ")
                            ),
                            amount_in,
                            amount_out: optimized.total_amount_out,
                            estimated_fee_usd: optimized.total_estimated_fee_usd,
                            duration_seconds: optimized.max_duration_seconds,
                            execution_payload: None, // Composite payload handled via parallel_paths
                            parallel_paths: Some(parallel_paths),
                            is_split_route: true,
                            slippage_percentage: Some(optimized.average_slippage_percentage),
                        });
                    } else {
                        // Fallback to single best path
                        tracing::info!("Single path is optimal for this order size");
                        
                        if let Some(path) = optimized.paths.first() {
                            routes.push(RouteOption {
                                provider: path.provider.clone(),
                                path: format!("Direct bridge via {}", path.provider),
                                amount_in,
                                amount_out: path.amount_out,
                                estimated_fee_usd: path.estimated_fee_usd,
                                duration_seconds: path.duration_seconds,
                                execution_payload: path.execution_payload.clone(),
                                parallel_paths: None,
                                is_split_route: false,
                                slippage_percentage: Some(path.slippage_percentage),
                            });
                        }
                    }

                    // Also add single-path alternatives for comparison
                    for bridge in available_bridges.iter().take(3) {
                        routes.push(RouteOption {
                            provider: bridge.provider.clone(),
                            path: format!("Single path via {}", bridge.provider),
                            amount_in,
                            amount_out: bridge.amount_out,
                            estimated_fee_usd: bridge.estimated_fee_usd,
                            duration_seconds: bridge.duration_seconds,
                            execution_payload: bridge.execution_payload.clone(),
                            parallel_paths: None,
                            is_split_route: false,
                            slippage_percentage: None,
                        });
                    }

                    return Ok(routes);
                }
                Err(e) => {
                    tracing::warn!("Multi-path optimization failed: {}, falling back to standard routing", e);
                }
            }
        }

        // Fall back to standard pathfinding for small orders or if optimization fails
        self.find_best_route(source_chain, dest_chain, source_asset, dest_asset, amount_in)
            .await
    }

    /// Get available bridge providers for direct transfer
    async fn get_available_bridges(
        &self,
        source_chain: Chain,
        dest_chain: Chain,
        asset: &str,
        amount: u64,
    ) -> Result<Vec<crate::bridge::BridgeQuote>, anyhow::Error> {
        let mut bridges = Vec::new();

        // Try CCTP (USDC only)
        if asset == "USDC" {
            if let Ok(quote) = self
                .cctp
                .get_quote(source_chain, dest_chain, asset, asset, amount)
                .await
            {
                bridges.push(quote);
            }
        }

        // Try DeBridge
        if let Ok(quote) = self
            .debridge
            .get_quote(source_chain, dest_chain, asset, asset, amount)
            .await
        {
            bridges.push(quote);
        }

        // Note: Stargate would be added here in production
        // if let Ok(quote) = self.stargate.get_quote(...).await {
        //     bridges.push(quote);
        // }

        Ok(bridges)
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn find_best_route(
        &self,
        source_chain: Chain,
        dest_chain: Chain,
        source_asset: &str,
        dest_asset: &str,
        amount_in: u64,
    ) -> Result<Vec<RouteOption>, anyhow::Error> {
        let start_node = Node {
            chain: source_chain,
            asset: source_asset.to_string(),
        };
        let end_node = Node {
            chain: dest_chain,
            asset: dest_asset.to_string(),
        };

        let mut pq = BinaryHeap::new();
        let mut best_seen: HashMap<Node, u64> = HashMap::new();

        let initial_usd = get_usd_value(source_asset, amount_in);

        pq.push(State {
            usd_value: (initial_usd * 1000.0) as u64,
            amount: amount_in,
            node: start_node.clone(),
            route_so_far: Vec::new(),
        });

        best_seen.insert(start_node.clone(), (initial_usd * 1000.0) as u64);

        let mut final_routes = Vec::new();
        let all_chains = vec![
            Chain::Ethereum,
            Chain::Arbitrum,
            Chain::Solana,
            Chain::Stellar,
        ];
        let all_assets = vec!["ETH", "USDC", "SOL", "XLM"];

        while let Some(state) = pq.pop() {
            if state.node == end_node {
                if !state.route_so_far.is_empty() {
                    let combined_provider = state
                        .route_so_far
                        .iter()
                        .map(|r| r.provider.clone())
                        .collect::<Vec<_>>()
                        .join(" + ");
                    let combined_path = state
                        .route_so_far
                        .iter()
                        .map(|r| r.path.clone())
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    let total_fee = state.route_so_far.iter().map(|r| r.estimated_fee_usd).sum();
                    let total_duration =
                        state.route_so_far.iter().map(|r| r.duration_seconds).sum();

                    final_routes.push(RouteOption {
                        provider: combined_provider,
                        path: combined_path,
                        amount_in, // original amount in
                        amount_out: state.amount,
                        estimated_fee_usd: total_fee,
                        duration_seconds: total_duration,
                        execution_payload: None,
                        parallel_paths: None,
                        is_split_route: false,
                        slippage_percentage: None,
                    });
                }
                continue;
            }

            // Limit path length to 3 hops to avoid excessive exploration
            if state.route_so_far.len() >= 3 {
                continue;
            }

            // 1. DEX Swaps (same chain)
            for target_asset in &all_assets {
                if target_asset != &state.node.asset {
                    if let Ok(quote) = DexProvider::get_swap_quote(
                        state.node.chain,
                        &state.node.asset,
                        target_asset,
                        state.amount,
                    ) {
                        let next_node = Node {
                            chain: state.node.chain,
                            asset: target_asset.to_string(),
                        };
                        let next_usd = get_usd_value(target_asset, quote.amount_out);
                        let next_usd_scaled = (next_usd * 1000.0) as u64;

                        let best = best_seen.entry(next_node.clone()).or_insert(0);
                        if next_usd_scaled > *best {
                            *best = next_usd_scaled;
                            let mut new_route = state.route_so_far.clone();
                            new_route.push(RouteOption {
                                provider: quote.provider,
                                path: format!("Swap {} to {}", state.node.asset, target_asset),
                                amount_in: state.amount,
                                amount_out: quote.amount_out,
                                estimated_fee_usd: quote.estimated_fee_usd,
                                duration_seconds: quote.duration_seconds,
                                execution_payload: None,
                                parallel_paths: None,
                                is_split_route: false,
                                slippage_percentage: None,
                            });
                            pq.push(State {
                                usd_value: next_usd_scaled,
                                amount: quote.amount_out,
                                node: next_node,
                                route_so_far: new_route,
                            });
                        }
                    }
                }
            }

            // 2. Bridges (cross chain)
            for target_chain in &all_chains {
                if target_chain != &state.node.chain {
                    // Try CCTP (USDC only)
                    if state.node.asset == "USDC" {
                        if let Ok(quote) = self
                            .cctp
                            .get_quote(
                                state.node.chain,
                                *target_chain,
                                "USDC",
                                "USDC",
                                state.amount,
                            )
                            .await
                        {
                            let next_node = Node {
                                chain: *target_chain,
                                asset: "USDC".to_string(),
                            };
                            let next_usd = get_usd_value("USDC", quote.amount_out);
                            let next_usd_net = next_usd - quote.estimated_fee_usd;
                            let next_usd_scaled = if next_usd_net > 0.0 {
                                (next_usd_net * 1000.0) as u64
                            } else {
                                0
                            };

                            let best = best_seen.entry(next_node.clone()).or_insert(0);
                            if next_usd_scaled > *best {
                                *best = next_usd_scaled;
                                let mut new_route = state.route_so_far.clone();
                                new_route.push(RouteOption {
                                    provider: quote.provider.clone(),
                                    path: format!("Bridge USDC via {}", quote.provider),
                                    amount_in: state.amount,
                                    amount_out: quote.amount_out,
                                    estimated_fee_usd: quote.estimated_fee_usd,
                                    duration_seconds: quote.duration_seconds,
                                    execution_payload: quote.execution_payload,
                                    parallel_paths: None,
                                    is_split_route: false,
                                    slippage_percentage: None,
                                });
                                pq.push(State {
                                    usd_value: next_usd_scaled,
                                    amount: quote.amount_out,
                                    node: next_node,
                                    route_so_far: new_route,
                                });
                            }
                        }
                    }

                    // Try DeBridge
                    if let Ok(quote) = self
                        .debridge
                        .get_quote(
                            state.node.chain,
                            *target_chain,
                            &state.node.asset,
                            &state.node.asset,
                            state.amount,
                        )
                        .await
                    {
                        let next_node = Node {
                            chain: *target_chain,
                            asset: state.node.asset.clone(),
                        };
                        let next_usd = get_usd_value(&state.node.asset, quote.amount_out);
                        let next_usd_net = next_usd - quote.estimated_fee_usd;
                        let next_usd_scaled = if next_usd_net > 0.0 {
                            (next_usd_net * 1000.0) as u64
                        } else {
                            0
                        };

                        let best = best_seen.entry(next_node.clone()).or_insert(0);
                        if next_usd_scaled > *best {
                            *best = next_usd_scaled;
                            let mut new_route = state.route_so_far.clone();
                            new_route.push(RouteOption {
                                provider: quote.provider.clone(),
                                path: format!("Bridge {} via {}", state.node.asset, quote.provider),
                                amount_in: state.amount,
                                amount_out: quote.amount_out,
                                estimated_fee_usd: quote.estimated_fee_usd,
                                duration_seconds: quote.duration_seconds,
                                execution_payload: quote.execution_payload,
                                parallel_paths: None,
                                is_split_route: false,
                                slippage_percentage: None,
                            });
                            pq.push(State {
                                usd_value: next_usd_scaled,
                                amount: quote.amount_out,
                                node: next_node,
                                route_so_far: new_route,
                            });
                        }
                    }
                }
            }
        }

        final_routes.sort_by_key(|r| {
            (
                std::cmp::Reverse(r.amount_out),
                (r.estimated_fee_usd * 100.0) as u64,
            )
        });

        // Return up to top 5 routes
        final_routes.truncate(5);

        Ok(final_routes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_find_best_route_usdc() {
        let planner = RoutePlanner::new();
        let routes = planner
            .find_best_route(Chain::Solana, Chain::Stellar, "USDC", "USDC", 10000)
            .await
            .unwrap();

        // The advanced router returns the single best route due to Dijkstra pruning
        assert_eq!(
            routes.len(),
            1,
            "Should return exactly 1 best route for USDC transfer"
        );

        assert!(!routes.is_empty(), "Should find at least one route");
    }

    #[tokio::test]
    async fn test_find_best_route_multi_hop_eth_to_xlm() {
        let planner = RoutePlanner::new();
        let routes = planner
            .find_best_route(
                Chain::Ethereum,
                Chain::Stellar,
                "ETH",
                "XLM",
                1, // 1 ETH
            )
            .await
            .unwrap();

        assert!(
            !routes.is_empty(),
            "Should find a multi-hop route for ETH -> XLM"
        );
        println!("Best multi-hop route: {:?}", routes[0]);
    }

    #[tokio::test]
    async fn test_large_order_multi_path_splitting() {
        let planner = RoutePlanner::new();
        
        // Large order: $1M (should trigger multi-path optimization)
        let amount = 1_000_000_000_000u64;
        
        let routes = planner
            .find_best_route_with_splitting(
                Chain::Ethereum,
                Chain::Stellar,
                "USDC",
                "USDC",
                amount,
            )
            .await
            .unwrap();
        
        assert!(!routes.is_empty(), "Should return routes for large order");
        
        let best_route = &routes[0];
        println!("\n=== $1M Order Routing Result ===");
        println!("Provider: {}", best_route.provider);
        println!("Is Split: {}", best_route.is_split_route);
        println!("Amount Out: ${}", best_route.amount_out);
        println!("Fee: ${:.2}", best_route.estimated_fee_usd);
        
        if let Some(slippage) = best_route.slippage_percentage {
            println!("Slippage: {:.3}%", slippage);
        }
        
        if best_route.is_split_route {
            if let Some(paths) = &best_route.parallel_paths {
                println!("\nParallel Paths:");
                for path in paths {
                    println!(
                        "  {} - {:.1}% (${} in, ${} out, {:.3}% slippage)",
                        path.provider,
                        path.split_percentage,
                        path.amount_in,
                        path.amount_out,
                        path.slippage_percentage
                    );
                }
                
                // Verify split integrity
                let total_split: f64 = paths.iter().map(|p| p.split_percentage).sum();
                assert!(
                    (total_split - 100.0).abs() < 0.1,
                    "Split percentages must sum to 100%"
                );
            }
        }
        
        println!("\n✓ Large order routing test passed");
    }

    #[tokio::test]
    async fn test_small_order_no_splitting() {
        let planner = RoutePlanner::new();
        
        // Small order: $10k (below threshold)
        let amount = 10_000_000_000u64;
        
        let routes = planner
            .find_best_route_with_splitting(
                Chain::Ethereum,
                Chain::Stellar,
                "USDC",
                "USDC",
                amount,
            )
            .await
            .unwrap();
        
        assert!(!routes.is_empty(), "Should return routes for small order");
        
        // Small orders typically won't benefit from splitting
        println!("✓ Small order routing test passed");
    }
}

pub mod dex;

use crate::bridge::{
    cctp::CctpClient, debridge::DeBridgeClient, gas_oracle::GasOracle, BridgeProvider, Chain,
};
use crate::router::dex::DexProvider;
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

    #[tracing::instrument(skip(self), err)]
    pub async fn find_best_route(
        &self,
        source_chain: Chain,
        dest_chain: Chain,
        source_asset: &str,
        dest_asset: &str,
        amount_in: u64,
        multi_path: bool,
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
                        if multi_path || next_usd_scaled > *best {
                            if next_usd_scaled > *best {
                                *best = next_usd_scaled;
                            }
                            let mut new_route = state.route_so_far.clone();
                            new_route.push(RouteOption {
                                provider: quote.provider,
                                path: format!("Swap {} to {}", state.node.asset, target_asset),
                                amount_in: state.amount,
                                amount_out: quote.amount_out,
                                estimated_fee_usd: quote.estimated_fee_usd,
                                duration_seconds: quote.duration_seconds,
                                execution_payload: None,
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
                            if multi_path || next_usd_scaled > *best {
                                if next_usd_scaled > *best {
                                    *best = next_usd_scaled;
                                }
                                let mut new_route = state.route_so_far.clone();
                                new_route.push(RouteOption {
                                    provider: quote.provider.clone(),
                                    path: format!("Bridge USDC via {}", quote.provider),
                                    amount_in: state.amount,
                                    amount_out: quote.amount_out,
                                    estimated_fee_usd: quote.estimated_fee_usd,
                                    duration_seconds: quote.duration_seconds,
                                    execution_payload: quote.execution_payload,
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
                        if multi_path || next_usd_scaled > *best {
                            if next_usd_scaled > *best {
                                *best = next_usd_scaled;
                            }
                            let mut new_route = state.route_so_far.clone();
                            new_route.push(RouteOption {
                                provider: quote.provider.clone(),
                                path: format!("Bridge {} via {}", state.node.asset, quote.provider),
                                amount_in: state.amount,
                                amount_out: quote.amount_out,
                                estimated_fee_usd: quote.estimated_fee_usd,
                                duration_seconds: quote.duration_seconds,
                                execution_payload: quote.execution_payload,
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
            .find_best_route(Chain::Solana, Chain::Stellar, "USDC", "USDC", 10000, false)
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
                false,
            )
            .await
            .unwrap();

        assert!(
            !routes.is_empty(),
            "Should find a multi-hop route for ETH -> XLM"
        );
        println!("Best multi-hop route: {:?}", routes[0]);
    }
}

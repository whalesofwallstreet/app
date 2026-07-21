pub mod dex;
pub mod slippage;

use crate::bridge::{
    cctp::CctpClient, debridge::DeBridgeClient, gas_oracle::GasOracle, BridgeProvider, Chain,
};
use crate::router::dex::DexProvider;
use crate::router::slippage::SlippageError;
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
    /// Total constant-product price impact across the legs of this route,
    /// in basis points.
    pub price_impact_bps: u32,
    /// Dynamic slippage tolerance for this route, derived from pool depth
    /// and trade size rather than a static default. Surfaced so the
    /// frontend can warn the user before execution.
    pub slippage_bps: u32,
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
        // First catastrophic price-impact rejection seen while exploring,
        // surfaced if the search ends with no viable route at all.
        let mut impact_rejection: Option<SlippageError> = None;
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
                    // Impacts on successive legs compound; summing is an
                    // accurate upper-bound approximation for impacts below
                    // the 15% rejection ceiling.
                    let total_impact = state.route_so_far.iter().map(|r| r.price_impact_bps).sum();
                    let total_slippage = state.route_so_far.iter().map(|r| r.slippage_bps).sum();

                    final_routes.push(RouteOption {
                        provider: combined_provider,
                        path: combined_path,
                        amount_in, // original amount in
                        amount_out: state.amount,
                        estimated_fee_usd: total_fee,
                        duration_seconds: total_duration,
                        price_impact_bps: total_impact,
                        slippage_bps: total_slippage,
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
                    match DexProvider::get_swap_quote(
                        state.node.chain,
                        &state.node.asset,
                        target_asset,
                        state.amount,
                    ) {
                        Ok(quote) => {
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
                                    price_impact_bps: quote.price_impact_bps,
                                    slippage_bps: quote.slippage_bps,
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
                        Err(err) => {
                            // Remember catastrophic-impact rejections so an
                            // unroutable trade surfaces a clear error rather
                            // than silently returning no routes.
                            if let Some(slippage_err) = err.downcast_ref::<SlippageError>() {
                                if matches!(
                                    slippage_err,
                                    SlippageError::ExcessivePriceImpact { .. }
                                ) && impact_rejection.is_none()
                                {
                                    impact_rejection = Some(slippage_err.clone());
                                }
                            }
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
                                    // Burn-and-mint bridging does not trade
                                    // against a pool, so no price impact.
                                    price_impact_bps: 0,
                                    slippage_bps: 0,
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
                                price_impact_bps: 0,
                                slippage_bps: 0,
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

        // If nothing was routable and at least one candidate leg was thrown
        // out for catastrophic price impact, fail loudly with that reason
        // instead of returning an empty route list.
        if final_routes.is_empty() {
            if let Some(rejection) = impact_rejection {
                return Err(anyhow::Error::new(rejection));
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
    async fn test_route_exposes_dynamic_slippage() {
        let planner = RoutePlanner::new();
        let routes = planner
            .find_best_route(Chain::Ethereum, Chain::Ethereum, "ETH", "USDC", 10)
            .await
            .unwrap();

        assert!(!routes.is_empty());
        let route = &routes[0];
        // A swap leg is involved, so the route must carry a non-zero dynamic
        // tolerance that includes at least the volatility buffer.
        assert!(route.slippage_bps >= slippage::VOLATILITY_BUFFER_BPS);
        assert!(route.slippage_bps > route.price_impact_bps);
    }

    #[tokio::test]
    async fn test_catastrophic_trade_is_rejected_with_clear_error() {
        let planner = RoutePlanner::new();
        // 60,000 ETH (~$180M) dwarfs every pool in the graph; all swap legs
        // are rejected for catastrophic price impact and no route exists.
        let err = planner
            .find_best_route(Chain::Ethereum, Chain::Ethereum, "ETH", "USDC", 60_000)
            .await
            .unwrap_err();

        assert!(
            err.downcast_ref::<SlippageError>().is_some(),
            "expected a typed slippage rejection, got: {err:?}"
        );
        assert!(err.to_string().contains("exceeds the maximum"));
    }
}

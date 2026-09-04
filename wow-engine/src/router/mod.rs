pub mod arbitrage;
pub mod dex;
pub mod slippage;

use crate::bridge::{
    cctp::CctpClient, debridge::DeBridgeClient, gas_oracle::GasOracle, price_oracle::PriceOracle,
    BridgeProvider, Chain,
};
use crate::config::AppConfig;
use crate::mempool::risk::HIGH_RISK_SLIPPAGE_PENALTY_BPS;
use crate::mempool::{PoolKey, PoolRiskRegistry};
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
    /// Pools currently flagged as under suspected front-running attack by
    /// the mempool listener (see [`crate::mempool`]). Checked on every DEX
    /// leg quoted below to widen that leg's slippage tolerance while the
    /// flag is live. Shared with (and only ever written by) the listener
    /// task when constructed via [`Self::with_risk_registry`]; a plain
    /// [`Self::new`] gets a private, always-empty registry, so routing
    /// behaves identically whether or not the mempool listener is running.
    risk_registry: Arc<PoolRiskRegistry>,
    /// Single source of truth for asset-to-USD pricing, shared with the
    /// `GasOracle` this planner's `cctp`/`debridge` clients use — so route
    /// pathfinding, DEX quotes, and gas-fee-in-USD estimates never disagree
    /// about what an asset is worth. See [`crate::bridge::price_oracle`].
    price_oracle: Arc<PriceOracle>,
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

async fn get_usd_value(price_oracle: &PriceOracle, asset: &str, amount: u64) -> f64 {
    (amount as f64) * price_oracle.get_price_usd(asset).await
}

impl RoutePlanner {
    pub fn new(config: Arc<AppConfig>) -> Self {
        Self::with_risk_registry(config, Arc::new(PoolRiskRegistry::new()))
    }

    /// Builds a planner that shares `risk_registry` with an external
    /// writer — in practice, `main.rs` wiring the same registry into both
    /// this planner and [`crate::mempool::listener::run_mempool_listener`]
    /// so a live front-running detection actually reaches quoting.
    pub fn with_risk_registry(
        config: Arc<AppConfig>,
        risk_registry: Arc<PoolRiskRegistry>,
    ) -> Self {
        let price_oracle = Arc::new(PriceOracle::new(config.clone()));
        let oracle = Arc::new(GasOracle::with_price_oracle(
            config.clone(),
            price_oracle.clone(),
        ));
        Self {
            debridge: DeBridgeClient::new(oracle.clone()),
            cctp: CctpClient::new(oracle, config),
            risk_registry,
            price_oracle,
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

        let initial_usd = get_usd_value(&self.price_oracle, source_asset, amount_in).await;

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
                        &self.price_oracle,
                        state.node.chain,
                        &state.node.asset,
                        target_asset,
                        state.amount,
                    )
                    .await
                    {
                        Ok(quote) => {
                            let next_node = Node {
                                chain: state.node.chain,
                                asset: target_asset.to_string(),
                            };
                            let next_usd =
                                get_usd_value(&self.price_oracle, target_asset, quote.amount_out)
                                    .await;
                            let next_usd_scaled = (next_usd * 1000.0) as u64;

                            // Widen this leg's slippage tolerance when the
                            // mempool listener has this pool flagged as
                            // under a suspected front-run/sandwich attack —
                            // see crate::mempool. This runs once per
                            // candidate edge in the search, not once per
                            // request, so the common case (no listener
                            // running, or one that hasn't flagged
                            // anything recently) is short-circuited via
                            // `is_empty()` before paying for the PoolKey
                            // allocation or a real cache lookup.
                            let slippage_bps = if self.risk_registry.is_empty() {
                                quote.slippage_bps
                            } else {
                                let pool =
                                    PoolKey::new(state.node.chain, &state.node.asset, target_asset);
                                if self.risk_registry.is_high_risk(&pool) {
                                    quote
                                        .slippage_bps
                                        .saturating_add(HIGH_RISK_SLIPPAGE_PENALTY_BPS)
                                } else {
                                    quote.slippage_bps
                                }
                            };

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
                                    price_impact_bps: quote.price_impact_bps,
                                    slippage_bps,
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
                            let next_usd =
                                get_usd_value(&self.price_oracle, "USDC", quote.amount_out).await;
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
                        let next_usd =
                            get_usd_value(&self.price_oracle, &state.node.asset, quote.amount_out)
                                .await;
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

    /// Builds a snapshot of the current liquidity graph — every DEX pair on
    /// every chain, plus every bridge route between chains — as
    /// [`arbitrage::ArbitrageEdge`]s, for negative-cycle (arbitrage)
    /// detection.
    ///
    /// Uses [`arbitrage::ARBITRAGE_REFERENCE_UNITS`] rather than a real
    /// user's trade size, so each edge approximates that pair's spot rate
    /// instead of a rate skewed by one particular trade's own price impact.
    /// Read-only against the same providers `find_best_route` uses, and
    /// shares no mutable state with it.
    pub async fn snapshot_arbitrage_edges(&self) -> Vec<arbitrage::ArbitrageEdge> {
        let all_chains = [
            Chain::Ethereum,
            Chain::Arbitrum,
            Chain::Solana,
            Chain::Stellar,
        ];
        let all_assets = ["ETH", "USDC", "SOL", "XLM"];
        let ref_units = arbitrage::ARBITRAGE_REFERENCE_UNITS;
        let mut edges = Vec::new();

        // Same-chain DEX swaps.
        for &chain in &all_chains {
            for &from in &all_assets {
                for &to in &all_assets {
                    if from == to {
                        continue;
                    }
                    if let Ok(quote) =
                        DexProvider::get_swap_quote(&self.price_oracle, chain, from, to, ref_units)
                            .await
                    {
                        edges.push(arbitrage::ArbitrageEdge {
                            from: arbitrage::GraphNode::new(chain, from),
                            to: arbitrage::GraphNode::new(chain, to),
                            rate: quote.amount_out as f64 / ref_units as f64,
                            label: format!("{} on {:?}: {} -> {}", quote.provider, chain, from, to),
                        });
                    }
                }
            }
        }

        // Cross-chain bridges: CCTP (USDC only) and DeBridge (any asset,
        // unchanged across the hop).
        for &source_chain in &all_chains {
            for &dest_chain in &all_chains {
                if source_chain == dest_chain {
                    continue;
                }

                if let Ok(quote) = self
                    .cctp
                    .get_quote(source_chain, dest_chain, "USDC", "USDC", ref_units)
                    .await
                {
                    edges.push(arbitrage::ArbitrageEdge {
                        from: arbitrage::GraphNode::new(source_chain, "USDC"),
                        to: arbitrage::GraphNode::new(dest_chain, "USDC"),
                        rate: quote.amount_out as f64 / ref_units as f64,
                        label: format!(
                            "{} bridge: USDC {:?} -> {:?}",
                            quote.provider, source_chain, dest_chain
                        ),
                    });
                }

                for &asset in &all_assets {
                    if let Ok(quote) = self
                        .debridge
                        .get_quote(source_chain, dest_chain, asset, asset, ref_units)
                        .await
                    {
                        edges.push(arbitrage::ArbitrageEdge {
                            from: arbitrage::GraphNode::new(source_chain, asset),
                            to: arbitrage::GraphNode::new(dest_chain, asset),
                            rate: quote.amount_out as f64 / ref_units as f64,
                            label: format!(
                                "{} bridge: {} {:?} -> {:?}",
                                quote.provider, asset, source_chain, dest_chain
                            ),
                        });
                    }
                }
            }
        }

        edges
    }

    /// Takes a snapshot of the live liquidity graph and checks it for an
    /// arbitrage loop. See [`arbitrage::detect_arbitrage`].
    pub async fn scan_for_arbitrage(&self) -> Option<arbitrage::ArbitrageDetected> {
        let edges = self.snapshot_arbitrage_edges().await;
        arbitrage::detect_arbitrage(&edges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every test in this module builds a real `RoutePlanner`, whose
    /// `PriceOracle`/`GasOracle` would otherwise make live, rate-limited
    /// calls to CoinGecko/Etherscan/Arbiscan on every distinct asset the
    /// Dijkstra/arbitrage search touches. Setting this (the same sentinel
    /// `benches/router_benchmarks.rs` already uses) makes every oracle
    /// resolve to its static fallback table instead — deterministic, and
    /// without which these tests went from milliseconds to 60+ seconds
    /// each once real network calls were on the line.
    fn use_fallback_pricing() {
        std::env::set_var("MOCK_GAS_ORACLE", "true");
    }

    #[tokio::test]
    async fn test_find_best_route_usdc() {
        use_fallback_pricing();
        let planner = RoutePlanner::new(Arc::new(AppConfig::default()));
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
        use_fallback_pricing();
        let planner = RoutePlanner::new(Arc::new(AppConfig::default()));
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

    #[tokio::test]
    async fn test_scan_for_arbitrage_finds_none_in_synthetic_liquidity_graph() {
        // Every DEX quote in this engine derives its reserves from the same
        // shared mock price oracle, and every bridge charges a fee, so no
        // loop through this mock liquidity graph can ever be profitable.
        // This is a regression guard: if a future change to the mock
        // pricing model (e.g. divergent price tables between providers)
        // accidentally introduces a free-money loop, this test catches it
        // rather than shipping it into `run_arbitrage_scanner`'s live scan.
        use_fallback_pricing();
        let planner = RoutePlanner::new(Arc::new(AppConfig::default()));
        let found = planner.scan_for_arbitrage().await;
        assert!(
            found.is_none(),
            "synthetic liquidity graph should have no arbitrage, found: {found:?}"
        );
    }

    #[tokio::test]
    async fn test_snapshot_arbitrage_edges_covers_dex_and_bridge_legs() {
        use_fallback_pricing();
        let planner = RoutePlanner::new(Arc::new(AppConfig::default()));
        let edges = planner.snapshot_arbitrage_edges().await;

        assert!(!edges.is_empty(), "should produce a non-trivial graph");
        assert!(
            edges.iter().any(|e| e.label.contains("bridge")),
            "should include at least one cross-chain bridge edge"
        );
        assert!(
            edges.iter().any(|e| !e.label.contains("bridge")),
            "should include at least one same-chain DEX edge"
        );
    }

    #[tokio::test]
    async fn high_risk_pool_flag_widens_the_routes_slippage_tolerance() {
        use_fallback_pricing();
        let config = Arc::new(AppConfig::default());
        let registry = Arc::new(PoolRiskRegistry::new());
        let planner = RoutePlanner::with_risk_registry(config, registry.clone());

        let baseline = planner
            .find_best_route(Chain::Ethereum, Chain::Ethereum, "ETH", "USDC", 1, false)
            .await
            .unwrap();
        let baseline_slippage = baseline[0].slippage_bps;

        registry.flag(
            PoolKey::new(Chain::Ethereum, "ETH", "USDC"),
            "test".to_string(),
        );

        let flagged = planner
            .find_best_route(Chain::Ethereum, Chain::Ethereum, "ETH", "USDC", 1, false)
            .await
            .unwrap();

        assert_eq!(
            flagged[0].slippage_bps,
            baseline_slippage + HIGH_RISK_SLIPPAGE_PENALTY_BPS
        );
    }

    #[tokio::test]
    async fn unflagged_pool_leaves_slippage_unchanged_across_calls() {
        use_fallback_pricing();
        let config = Arc::new(AppConfig::default());
        let planner = RoutePlanner::new(config);

        let first = planner
            .find_best_route(Chain::Ethereum, Chain::Ethereum, "ETH", "USDC", 1, false)
            .await
            .unwrap();
        let second = planner
            .find_best_route(Chain::Ethereum, Chain::Ethereum, "ETH", "USDC", 1, false)
            .await
            .unwrap();

        assert_eq!(first[0].slippage_bps, second[0].slippage_bps);
    }
}

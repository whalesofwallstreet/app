use crate::bridge::{Chain, debridge::DeBridgeClient, cctp::CctpClient, BridgeProvider};
use serde::{Deserialize, Serialize};

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

impl RoutePlanner {
    pub fn new() -> Self {
        Self {
            debridge: DeBridgeClient::new(),
            cctp: CctpClient::new(),
        }
    }

    pub async fn find_best_route(
        &self,
        source_chain: Chain,
        dest_chain: Chain,
        source_asset: &str,
        dest_asset: &str,
        amount_in: u64,
    ) -> Result<Vec<RouteOption>, anyhow::Error> {
        let mut routes = Vec::new();

        // 1. Gather quote from deBridge DLN
        if let Ok(quote) = self.debridge.get_quote(source_chain, dest_chain, source_asset, dest_asset, amount_in).await {
            routes.push(RouteOption {
                provider: quote.provider,
                path: format!("{} -> {}", source_chain, dest_chain),
                amount_in: quote.amount_in,
                amount_out: quote.amount_out,
                estimated_fee_usd: quote.estimated_fee_usd,
                duration_seconds: quote.duration_seconds,
                execution_payload: quote.execution_payload,
            });
        }

        // 2. Gather quote from Circle CCTP (for USDC asset pairs)
        let is_usdc_pair = source_asset.to_uppercase().contains("USDC") 
            && dest_asset.to_uppercase().contains("USDC");
            
        if is_usdc_pair {
            if let Ok(quote) = self.cctp.get_quote(source_chain, dest_chain, source_asset, dest_asset, amount_in).await {
                routes.push(RouteOption {
                    provider: quote.provider,
                    path: format!("{} -(Native CCTP)-> {}", source_chain, dest_chain),
                    amount_in: quote.amount_in,
                    amount_out: quote.amount_out,
                    estimated_fee_usd: quote.estimated_fee_usd,
                    duration_seconds: quote.duration_seconds,
                    execution_payload: quote.execution_payload,
                });
            }
        }

        // Sort routes: highest amount_out first, then lowest estimated_fee_usd.
        // We use a scaled key to bypass floating-point comparisons for sorting.
        routes.sort_by_key(|r| {
            (
                std::cmp::Reverse(r.amount_out),
                (r.estimated_fee_usd * 100.0) as u64,
            )
        });

        Ok(routes)
    }
}

impl Default for RoutePlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_find_best_route_usdc() {
        let planner = RoutePlanner::new();
        let routes = planner.find_best_route(
            Chain::Solana,
            Chain::Stellar,
            "USDC",
            "USDC",
            10000,
        ).await.unwrap();

        assert_eq!(routes.len(), 2, "Should return exactly 2 routes for USDC transfer");
        
        // Route 0 must be Circle CCTP due to 1:1 burn/mint output (10000 out)
        assert_eq!(routes[0].provider, "Circle CCTP");
        assert_eq!(routes[0].amount_out, 10000);

        // Route 1 must be deBridge DLN due to 0.1% protocol fee (9990 out)
        assert_eq!(routes[1].provider, "deBridge DLN");
        assert_eq!(routes[1].amount_out, 9990);
    }

    #[tokio::test]
    async fn test_find_best_route_non_usdc() {
        let planner = RoutePlanner::new();
        let routes = planner.find_best_route(
            Chain::Solana,
            Chain::Stellar,
            "SOL",
            "XLM",
            10000,
        ).await.unwrap();

        // Non-USDC route should only use deBridge DLN (since Circle CCTP is USDC only)
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].provider, "deBridge DLN");
        assert_eq!(routes[0].amount_out, 9990);
    }

    #[tokio::test]
    async fn test_routes_sorting_by_amount_out_and_fee() {
        // Prepare dummy route options to test sorting logic
        let mut routes = vec![
            RouteOption {
                provider: "Provider B".to_string(),
                path: "A -> B".to_string(),
                amount_in: 100,
                amount_out: 95,
                estimated_fee_usd: 1.50,
                duration_seconds: 60,
                execution_payload: None,
            },
            RouteOption {
                provider: "Provider A".to_string(),
                path: "A -> B".to_string(),
                amount_in: 100,
                amount_out: 98,
                estimated_fee_usd: 2.00,
                duration_seconds: 60,
                execution_payload: None,
            },
            RouteOption {
                provider: "Provider C (Tiebreaker)".to_string(),
                path: "A -> B".to_string(),
                amount_in: 100,
                amount_out: 98,
                estimated_fee_usd: 0.50, // lowest fee should tiebreak and win first place
                duration_seconds: 30,
                execution_payload: None,
            },
        ];

        // Apply sorting key mechanism used in find_best_route
        routes.sort_by_key(|r| {
            (
                std::cmp::Reverse(r.amount_out),
                (r.estimated_fee_usd * 100.0) as u64,
            )
        });

        assert_eq!(routes[0].provider, "Provider C (Tiebreaker)");
        assert_eq!(routes[1].provider, "Provider A");
        assert_eq!(routes[2].provider, "Provider B");
    }
}

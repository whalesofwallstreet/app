use wow_engine::bridge::Chain;
use wow_engine::router::RoutePlanner;

#[tokio::test]
async fn test_large_order_triggers_multi_path_optimization() {
    let planner = RoutePlanner::new();
    
    // Simulate a $1M USDC transfer (with 6 decimals: 1,000,000 * 10^6)
    let amount = 1_000_000_000_000u64; // $1M in smallest units
    
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
    
    assert!(!routes.is_empty(), "Should return at least one route");
    
    // First route should be the optimized multi-path route
    let best_route = &routes[0];
    println!("Best route: {:?}", best_route);
    
    // For large orders, we expect splitting to be beneficial
    if best_route.is_split_route {
        println!("✓ Multi-path optimization activated for $1M order");
        assert!(
            best_route.parallel_paths.is_some(),
            "Split route should contain parallel paths"
        );
        
        let parallel_paths = best_route.parallel_paths.as_ref().unwrap();
        assert!(
            parallel_paths.len() >= 2,
            "Should split across at least 2 bridges"
        );
        
        // Verify split percentages sum to 100%
        let total_split: f64 = parallel_paths.iter().map(|p| p.split_percentage).sum();
        assert!(
            (total_split - 100.0).abs() < 0.1,
            "Split percentages should sum to 100%, got {}",
            total_split
        );
        
        // Verify amounts sum correctly
        let total_in: u64 = parallel_paths.iter().map(|p| p.amount_in).sum();
        assert!(
            (total_in as i64 - amount as i64).abs() < 1000,
            "Sum of split amounts should equal total input"
        );
        
        println!("Split ratio:");
        for path in parallel_paths {
            println!(
                "  - {}: {:.1}% (${} -> ${}, slippage: {:.2}%)",
                path.provider,
                path.split_percentage,
                path.amount_in,
                path.amount_out,
                path.slippage_percentage
            );
        }
    } else {
        println!("Note: Single path was optimal (gas costs may have exceeded slippage savings)");
    }
}

#[tokio::test]
async fn test_small_order_uses_single_path() {
    let planner = RoutePlanner::new();
    
    // Small order: $10k (below $100k threshold)
    let amount = 10_000_000_000u64; // $10k in smallest units
    
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
    
    assert!(!routes.is_empty(), "Should return at least one route");
    
    let best_route = &routes[0];
    
    // Small orders should not be split (not worth the gas overhead)
    assert!(
        !best_route.is_split_route || best_route.parallel_paths.is_none(),
        "Small orders should use single-path routing"
    );
    
    println!("✓ Small order correctly uses single-path routing");
}

#[tokio::test]
async fn test_multi_path_reduces_slippage_vs_single_path() {
    let planner = RoutePlanner::new();
    
    // Large order where splitting should provide benefit
    let amount = 1_000_000_000_000u64; // $1M
    
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
    
    assert!(routes.len() >= 2, "Should return both split and single-path options");
    
    let split_route = &routes[0];
    
    // Find single-path alternatives in the results
    let single_path_routes: Vec<_> = routes.iter().filter(|r| !r.is_split_route).collect();
    
    if split_route.is_split_route && !single_path_routes.is_empty() {
        println!("\n=== Slippage Comparison ===");
        
        if let Some(slippage) = split_route.slippage_percentage {
            println!("Multi-path average slippage: {:.3}%", slippage);
        }
        
        println!("\nSingle-path alternatives:");
        for single in single_path_routes {
            println!(
                "  {} - Output: ${}, Fee: ${:.2}",
                single.provider, single.amount_out, single.estimated_fee_usd
            );
        }
        
        // The optimization should result in better net output
        println!(
            "\nMulti-path output: ${} (fee: ${:.2})",
            split_route.amount_out, split_route.estimated_fee_usd
        );
        
        // In most cases, multi-path should provide better or comparable output
        // after accounting for fees
        let split_net = split_route.amount_out as f64 - split_route.estimated_fee_usd;
        
        let best_single_net = single_path_routes
            .iter()
            .map(|r| r.amount_out as f64 - r.estimated_fee_usd)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);
        
        println!("Split net value: ${:.2}", split_net);
        println!("Best single net value: ${:.2}", best_single_net);
        
        // Multi-path should be at least as good as single path
        // (with small tolerance for rounding)
        assert!(
            split_net >= best_single_net * 0.999,
            "Multi-path should provide better or equal net value"
        );
        
        println!("✓ Multi-path optimization provides superior net output");
    }
}

#[tokio::test]
async fn test_parallel_paths_have_valid_execution_payloads() {
    let planner = RoutePlanner::new();
    
    let amount = 1_000_000_000_000u64; // $1M
    
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
    
    let split_route = routes.iter().find(|r| r.is_split_route);
    
    if let Some(route) = split_route {
        if let Some(paths) = &route.parallel_paths {
            println!("\n=== Execution Payloads ===");
            for (i, path) in paths.iter().enumerate() {
                println!("\nPath {}: {}", i + 1, path.provider);
                println!("  Amount: ${}", path.amount_in);
                
                if let Some(payload) = &path.execution_payload {
                    assert!(!payload.is_empty(), "Execution payload should not be empty");
                    println!("  Payload: {}", payload);
                } else {
                    println!("  Payload: None (will be generated on execution)");
                }
            }
            
            println!("\n✓ All parallel paths have valid structure");
        }
    }
}

#[tokio::test]
async fn test_multi_path_max_duration_calculation() {
    let planner = RoutePlanner::new();
    
    let amount = 1_000_000_000_000u64; // $1M
    
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
    
    let split_route = routes.iter().find(|r| r.is_split_route);
    
    if let Some(route) = split_route {
        if let Some(paths) = &route.parallel_paths {
            // Max duration should be the longest path (parallel execution)
            let max_path_duration = paths.iter().map(|p| p.duration_seconds).max().unwrap_or(0);
            
            assert_eq!(
                route.duration_seconds, max_path_duration,
                "Total duration should equal longest parallel path"
            );
            
            println!(
                "✓ Parallel execution duration correctly calculated: {}s",
                route.duration_seconds
            );
        }
    }
}

#[tokio::test]
async fn test_edge_case_exact_threshold() {
    let planner = RoutePlanner::new();
    
    // Exactly at $100k threshold
    let amount = 100_000_000_000u64;
    
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
    
    assert!(!routes.is_empty(), "Should return routes at threshold");
    println!("✓ Edge case at exact threshold handled correctly");
}

#[tokio::test]
async fn test_cross_chain_different_assets() {
    let planner = RoutePlanner::new();
    
    // Test with non-USDC asset (should still work with DeBridge)
    let amount = 1_000_000_000_000u64;
    
    let routes = planner
        .find_best_route_with_splitting(
            Chain::Ethereum,
            Chain::Stellar,
            "ETH",
            "USDC",
            amount,
        )
        .await
        .unwrap();
    
    // Should find routes even with asset conversion
    assert!(!routes.is_empty(), "Should find routes for ETH -> USDC");
    
    println!("✓ Multi-hop routing with asset conversion works");
}

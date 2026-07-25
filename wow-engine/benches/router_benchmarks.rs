use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;
use wow_engine::bridge::Chain;
use wow_engine::router::RoutePlanner;

fn bench_router(c: &mut Criterion) {
    // Set mock gas oracle environment variable to bypass external HTTP calls
    std::env::set_var("MOCK_GAS_ORACLE", "true");

    let rt = Runtime::new().unwrap();
    let _guard = rt.enter();
    let planner = RoutePlanner::new();

    let mut group = c.benchmark_group("routing_engine");
    group.sample_size(100);

    // 1. Solana -> Stellar USDC (USDC-to-USDC) - Single-path Dijkstra Routing
    group.bench_function("dijkstra_single_path_usdc", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = planner
                .find_best_route(
                    Chain::Solana,
                    Chain::Stellar,
                    "USDC",
                    "USDC",
                    10000,
                    false, // multi_path = false (single-path Dijkstra)
                )
                .await;
        });
    });

    // 2. Solana -> Stellar USDC (USDC-to-USDC) - Multi-path / Max-flow Routing
    group.bench_function("multipath_search_usdc", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = planner
                .find_best_route(
                    Chain::Solana,
                    Chain::Stellar,
                    "USDC",
                    "USDC",
                    10000000, // high liquidity/amount
                    true, // multi_path = true
                )
                .await;
        });
    });

    // 3. Ethereum -> Stellar XLM (Multi-hop cross-chain / multi-asset) - Single-path Dijkstra Routing
    group.bench_function("dijkstra_single_path_multi_hop", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = planner
                .find_best_route(
                    Chain::Ethereum,
                    Chain::Stellar,
                    "ETH",
                    "XLM",
                    1,
                    false, // multi_path = false
                )
                .await;
        });
    });

    // 4. Ethereum -> Stellar XLM (Multi-hop cross-chain / multi-asset) - Multi-path / Max-flow Routing
    group.bench_function("multipath_search_multi_hop", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = planner
                .find_best_route(
                    Chain::Ethereum,
                    Chain::Stellar,
                    "ETH",
                    "XLM",
                    1,
                    true, // multi_path = true
                )
                .await;
        });
    });

    group.finish();
}

criterion_group!(benches, bench_router);
criterion_main!(benches);

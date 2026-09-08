use std::time::Instant;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use wow_engine::wal::{WalEntry, WalManager};

#[tokio::test]
async fn test_wal_append_and_replay_reconstructs_state() {
    let temp_dir = std::env::temp_dir().join(format!("test_wal_{}", uuid::Uuid::new_v4()));
    let wal = WalManager::new(&temp_dir);

    let route_id = "route_12345".to_string();
    let user_id = "user_9999".to_string();

    let entry1 = WalEntry::RouteCreated {
        id: route_id.clone(),
        user_id: user_id.clone(),
        source_chain: "Ethereum".to_string(),
        dest_chain: "Arbitrum".to_string(),
        source_asset: "USDC".to_string(),
        dest_asset: "USDC".to_string(),
        amount_in: 1000,
        amount_out: 998,
        provider: "cctp".to_string(),
        path: "Ethereum->Arbitrum".to_string(),
        timestamp: 1000,
    };

    let entry2 = WalEntry::RouteStatusUpdated {
        id: route_id.clone(),
        status: "PENDING_ATTESTATION".to_string(),
        timestamp: 1005,
    };

    let entry3 = WalEntry::RouteCompleted {
        id: route_id.clone(),
        amount_out: Some(999),
        timestamp: 1010,
    };

    wal.append(&entry1).await.unwrap();
    wal.append(&entry2).await.unwrap();
    wal.append(&entry3).await.unwrap();

    let state_map = wal.replay().await.unwrap();
    assert_eq!(state_map.len(), 1);

    let state = state_map.get(&route_id).expect("route state should exist");
    assert_eq!(state.id, route_id);
    assert_eq!(state.user_id, user_id);
    assert_eq!(state.status, "COMPLETED");
    assert_eq!(state.amount_out, 999);
    assert_eq!(state.updated_at, 1010);

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
async fn test_wal_handles_truncated_trailing_entry_gracefully() {
    let temp_dir = std::env::temp_dir().join(format!("test_wal_trunc_{}", uuid::Uuid::new_v4()));
    let wal = WalManager::new(&temp_dir);

    let route_id = "route_trunc_1".to_string();
    let entry1 = WalEntry::RouteCreated {
        id: route_id.clone(),
        user_id: "user_1".to_string(),
        source_chain: "Ethereum".to_string(),
        dest_chain: "Solana".to_string(),
        source_asset: "ETH".to_string(),
        dest_asset: "SOL".to_string(),
        amount_in: 100,
        amount_out: 2000,
        provider: "debridge".to_string(),
        path: "ETH->SOL".to_string(),
        timestamp: 100,
    };

    wal.append(&entry1).await.unwrap();

    // Manually append partial length bytes / corrupted trailing bytes at the end of active.wal
    let active_path = wal.active_file_path();
    let mut file = OpenOptions::new()
        .append(true)
        .open(&active_path)
        .await
        .unwrap();

    // Write incomplete length header (2 bytes instead of 4)
    file.write_all(&[0x10, 0x00]).await.unwrap();
    file.flush().await.unwrap();
    drop(file);

    // Replay should recover entry1 cleanly and ignore the trailing incomplete byte sequence
    let state_map = wal.replay().await.unwrap();
    assert_eq!(state_map.len(), 1);
    let state = state_map.get(&route_id).unwrap();
    assert_eq!(state.status, "CREATED");

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
async fn test_wal_rotation_and_archiving() {
    let temp_dir = std::env::temp_dir().join(format!("test_wal_rotate_{}", uuid::Uuid::new_v4()));
    // Set small max file size (150 bytes) to force rotation, with sufficient max_archives to retain all files
    let wal = WalManager::new(&temp_dir)
        .with_max_file_size(150)
        .with_max_archives(20);

    for i in 0..10 {
        let entry = WalEntry::RouteCreated {
            id: format!("route_{}", i),
            user_id: format!("user_{}", i),
            source_chain: "Ethereum".to_string(),
            dest_chain: "Polygon".to_string(),
            source_asset: "USDC".to_string(),
            dest_asset: "USDC".to_string(),
            amount_in: 500,
            amount_out: 500,
            provider: "cctp".to_string(),
            path: "USDC".to_string(),
            timestamp: 1000 + i as u64,
        };
        wal.append(&entry).await.unwrap();
    }

    // Replay across all archived + active logs
    let state_map = wal.replay().await.unwrap();
    // All 10 routes should be reconstructed across rotated files
    assert_eq!(state_map.len(), 10);

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
async fn test_wal_archive_pruning_bounds_disk_usage() {
    let temp_dir = std::env::temp_dir().join(format!("test_wal_prune_{}", uuid::Uuid::new_v4()));
    // Set max_archives = 2
    let wal = WalManager::new(&temp_dir)
        .with_max_file_size(150)
        .with_max_archives(2);

    for i in 0..10 {
        let entry = WalEntry::RouteCreated {
            id: format!("route_prune_{}", i),
            user_id: format!("user_{}", i),
            source_chain: "Ethereum".to_string(),
            dest_chain: "Polygon".to_string(),
            source_asset: "USDC".to_string(),
            dest_asset: "USDC".to_string(),
            amount_in: 500,
            amount_out: 500,
            provider: "cctp".to_string(),
            path: "USDC".to_string(),
            timestamp: 1000 + i as u64,
        };
        wal.append(&entry).await.unwrap();
    }

    // Count archive files in directory
    let mut read_dir = fs::read_dir(&temp_dir).await.unwrap();
    let mut archive_count = 0;
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        if let Some(filename) = entry.file_name().to_str() {
            if filename.starts_with("wal_") && filename.ends_with(".archive") {
                archive_count += 1;
            }
        }
    }

    assert!(archive_count <= 2, "Archive count should be bounded by max_archives=2, got {}", archive_count);

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
async fn test_wal_latency_benchmark_under_2ms() {
    let temp_dir = std::env::temp_dir().join(format!("test_wal_bench_{}", uuid::Uuid::new_v4()));
    let wal = WalManager::new(&temp_dir);

    let iterations = 50;
    let start = Instant::now();

    for i in 0..iterations {
        let entry = WalEntry::RouteCreated {
            id: format!("route_bench_{}", i),
            user_id: "user_bench".to_string(),
            source_chain: "Ethereum".to_string(),
            dest_chain: "Arbitrum".to_string(),
            source_asset: "ETH".to_string(),
            dest_asset: "WETH".to_string(),
            amount_in: 10,
            amount_out: 10,
            provider: "dex".to_string(),
            path: "ETH->WETH".to_string(),
            timestamp: 2000 + i as u64,
        };
        wal.append(&entry).await.unwrap();
    }

    let elapsed = start.elapsed();
    let avg_latency_ms = (elapsed.as_secs_f64() * 1000.0) / (iterations as f64);
    println!("Average WAL append latency: {:.3} ms", avg_latency_ms);

    assert!(
        avg_latency_ms < 2.0,
        "WAL append latency must be under 2ms, got {:.3}ms",
        avg_latency_ms
    );

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir).await;
}

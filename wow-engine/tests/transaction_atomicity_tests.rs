use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use uuid::Uuid;
use wow_engine::db::models::{RouteExecution, RouteExecutionInput};
use wow_engine::db::operations::{AnchorTransactionRepo, RouteExecutionRepo, UserQuotaRepo};

async fn setup_db() -> Result<sqlx::postgres::PgPool, sqlx::Error> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/wow_engine_test".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await.ok();

    Ok(pool)
}

async fn cleanup_db(pool: &sqlx::postgres::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("TRUNCATE anchor_transactions CASCADE")
        .execute(pool)
        .await
        .ok();
    sqlx::query("TRUNCATE user_history CASCADE")
        .execute(pool)
        .await
        .ok();
    sqlx::query("TRUNCATE user_quotas CASCADE")
        .execute(pool)
        .await
        .ok();
    sqlx::query("TRUNCATE route_executions CASCADE")
        .execute(pool)
        .await
        .ok();
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_atomic_route_execution_commit() {
    let pool = match setup_db().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping test: {}", e);
            return;
        }
    };

    let user_id = Uuid::now_v7();
    let route_input = RouteExecutionInput {
        user_id,
        source_chain: "Ethereum".to_string(),
        dest_chain: "Stellar".to_string(),
        source_asset: "USDC".to_string(),
        dest_asset: "USDC".to_string(),
        amount_in: 100_000_000,
        amount_out: 99_500_000,
        provider: "CCTP".to_string(),
        path: "Ethereum -> Stellar via CCTP".to_string(),
        estimated_fee_usd: 5.0,
    };

    // Begin transaction and perform all operations
    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    let route_result = RouteExecutionRepo::create_with_history(
        &mut tx,
        route_input.clone(),
        "route_created",
        "Route execution initiated",
    )
    .await;

    assert!(route_result.is_ok(), "Route creation should succeed");
    let route = route_result.unwrap();

    let quota_result = UserQuotaRepo::get_or_create(&mut tx, user_id).await;
    assert!(quota_result.is_ok(), "Quota creation should succeed");

    let _quota = quota_result.unwrap();
    let update_quota_result = UserQuotaRepo::update_usage(&mut tx, user_id, 500.0).await;
    assert!(update_quota_result.is_ok(), "Quota update should succeed");

    let anchor_result = AnchorTransactionRepo::create(
        &mut tx,
        user_id,
        route.id,
        "anchor.example.com",
        "tx_sep24_123",
        "https://anchor.example.com/interactive",
    )
    .await;
    assert!(
        anchor_result.is_ok(),
        "Anchor transaction creation should succeed"
    );

    // Commit the transaction
    tx.commit().await.expect("Failed to commit transaction");

    // Verify all records were persisted
    let persisted_route =
        sqlx::query_as::<_, RouteExecution>("SELECT * FROM route_executions WHERE id = $1")
            .bind(route.id)
            .fetch_one(&pool)
            .await;

    assert!(
        persisted_route.is_ok(),
        "Route should be persisted after commit"
    );

    let history_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM user_history WHERE route_execution_id = $1")
            .bind(route.id)
            .fetch_one(&pool)
            .await
            .expect("Failed to count history");

    assert_eq!(
        history_count.0, 1,
        "User history should be created for the route"
    );

    let quota_check: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_quotas WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to count quotas");

    assert_eq!(quota_check.0, 1, "Quota record should exist");

    let anchor_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM anchor_transactions WHERE route_execution_id = $1")
            .bind(route.id)
            .fetch_one(&pool)
            .await
            .expect("Failed to count anchor transactions");

    assert_eq!(anchor_count.0, 1, "Anchor transaction should be created");

    let _ = cleanup_db(&pool).await;
}

#[tokio::test]
#[ignore]
async fn test_atomic_route_execution_rollback() {
    let pool = match setup_db().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping test: {}", e);
            return;
        }
    };

    let user_id = Uuid::now_v7();
    let route_input = RouteExecutionInput {
        user_id,
        source_chain: "Ethereum".to_string(),
        dest_chain: "Stellar".to_string(),
        source_asset: "USDC".to_string(),
        dest_asset: "USDC".to_string(),
        amount_in: 100_000_000,
        amount_out: 99_500_000,
        provider: "CCTP".to_string(),
        path: "Ethereum -> Stellar via CCTP".to_string(),
        estimated_fee_usd: 5.0,
    };

    // Begin transaction and perform operations
    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    let route_result = RouteExecutionRepo::create_with_history(
        &mut tx,
        route_input.clone(),
        "route_created",
        "Route execution initiated",
    )
    .await;

    assert!(route_result.is_ok());
    let route = route_result.unwrap();

    let quota_result = UserQuotaRepo::get_or_create(&mut tx, user_id).await;
    assert!(quota_result.is_ok());

    let _ = UserQuotaRepo::update_usage(&mut tx, user_id, 500.0).await;

    let anchor_result = AnchorTransactionRepo::create(
        &mut tx,
        user_id,
        route.id,
        "anchor.example.com",
        "tx_sep24_123",
        "https://anchor.example.com/interactive",
    )
    .await;
    assert!(anchor_result.is_ok());

    // Simulate error by rolling back the transaction
    tx.rollback().await.expect("Failed to rollback transaction");

    // Verify NO records were persisted
    let persisted_route: Result<RouteExecution, _> =
        sqlx::query_as("SELECT * FROM route_executions WHERE id = $1")
            .bind(route.id)
            .fetch_one(&pool)
            .await;

    assert!(
        persisted_route.is_err(),
        "Route should NOT be persisted after rollback"
    );

    let history_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM user_history WHERE route_execution_id = $1")
            .bind(route.id)
            .fetch_one(&pool)
            .await
            .expect("Failed to count history");

    assert_eq!(
        history_count.0, 0,
        "User history should NOT be created after rollback"
    );

    let quota_check: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_quotas WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to count quotas");

    assert_eq!(
        quota_check.0, 0,
        "Quota record should NOT be persisted after rollback"
    );

    let anchor_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM anchor_transactions WHERE route_execution_id = $1")
            .bind(route.id)
            .fetch_one(&pool)
            .await
            .expect("Failed to count anchor transactions");

    assert_eq!(
        anchor_count.0, 0,
        "Anchor transaction should NOT be created after rollback"
    );

    let _ = cleanup_db(&pool).await;
}

#[tokio::test]
#[ignore]
async fn test_concurrent_route_executions() {
    let pool = match setup_db().await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping test: {}", e);
            return;
        }
    };

    // Simulate 5 concurrent users executing routes
    let mut handles = vec![];

    for i in 0..5 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let user_id = Uuid::now_v7();
            let route_input = RouteExecutionInput {
                user_id,
                source_chain: "Ethereum".to_string(),
                dest_chain: "Stellar".to_string(),
                source_asset: "USDC".to_string(),
                dest_asset: "USDC".to_string(),
                amount_in: 50_000_000 + (i * 10_000_000) as i64,
                amount_out: 49_500_000 + (i * 9_500_000) as i64,
                provider: format!("Provider_{}", i),
                path: format!("Route {}", i),
                estimated_fee_usd: 2.5 + (i as f64),
            };

            let mut tx = pool_clone
                .begin()
                .await
                .expect("Failed to begin transaction");

            let route = RouteExecutionRepo::create_with_history(
                &mut tx,
                route_input,
                "route_created",
                "Concurrent route execution",
            )
            .await
            .expect("Failed to create route");

            let _ = UserQuotaRepo::get_or_create(&mut tx, user_id).await;
            let _ = UserQuotaRepo::update_usage(&mut tx, user_id, 250.0).await;

            tx.commit().await.expect("Failed to commit transaction");

            route.id
        });

        handles.push(handle);
    }

    // Wait for all concurrent operations to complete
    let mut route_ids = vec![];
    for handle in handles {
        let route_id = handle.await.expect("Task panicked");
        route_ids.push(route_id);
    }

    // Verify all routes were persisted independently
    for route_id in route_ids {
        let result =
            sqlx::query_as::<_, RouteExecution>("SELECT * FROM route_executions WHERE id = $1")
                .bind(route_id)
                .fetch_one(&pool)
                .await;

        assert!(
            result.is_ok(),
            "All concurrent routes should be persisted independently"
        );
    }

    // Verify quota isolation
    let quota_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_quotas")
        .fetch_one(&pool)
        .await
        .expect("Failed to count quotas");

    assert_eq!(
        quota_count.0, 5,
        "Each user should have their own quota record"
    );

    let _ = cleanup_db(&pool).await;
}

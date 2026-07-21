use crate::db::models::{
    AnchorTransaction, RouteExecution, RouteExecutionInput, UserHistory, UserQuota,
};
use chrono::Utc;
use sqlx::Transaction;
use uuid::Uuid;

pub struct RouteExecutionRepo;

impl RouteExecutionRepo {
    pub async fn create_with_history(
        tx: &mut Transaction<'_, sqlx::Postgres>,
        input: RouteExecutionInput,
        history_action: &str,
        history_details: &str,
    ) -> Result<RouteExecution, sqlx::Error> {
        let id = Uuid::new_v7();
        let now = Utc::now();

        let route = sqlx::query_as::<_, RouteExecution>(
            r#"
            INSERT INTO route_executions
            (id, user_id, source_chain, dest_chain, source_asset, dest_asset,
             amount_in, amount_out, provider, path, estimated_fee_usd, status, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(input.user_id)
        .bind(&input.source_chain)
        .bind(&input.dest_chain)
        .bind(&input.source_asset)
        .bind(&input.dest_asset)
        .bind(input.amount_in)
        .bind(input.amount_out)
        .bind(&input.provider)
        .bind(&input.path)
        .bind(input.estimated_fee_usd)
        .bind("pending")
        .bind(now)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO user_history (id, user_id, route_execution_id, action, details, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(Uuid::new_v7())
        .bind(input.user_id)
        .bind(id)
        .bind(history_action)
        .bind(history_details)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        Ok(route)
    }

    pub async fn update_status(
        tx: &mut Transaction<'_, sqlx::Postgres>,
        id: Uuid,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE route_executions
            SET status = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(status)
        .bind(now)
        .bind(id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

pub struct UserHistoryRepo;

impl UserHistoryRepo {
    pub async fn create(
        tx: &mut Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        route_execution_id: Uuid,
        action: &str,
        details: &str,
    ) -> Result<UserHistory, sqlx::Error> {
        let id = Uuid::new_v7();
        let now = Utc::now();

        sqlx::query_as::<_, UserHistory>(
            r#"
            INSERT INTO user_history (id, user_id, route_execution_id, action, details, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(route_execution_id)
        .bind(action)
        .bind(details)
        .bind(now)
        .fetch_one(&mut **tx)
        .await
    }
}

pub struct UserQuotaRepo;

impl UserQuotaRepo {
    pub async fn get_or_create(
        tx: &mut Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
    ) -> Result<UserQuota, sqlx::Error> {
        let now = Utc::now();
        let reset_at = now + chrono::Duration::days(1);

        sqlx::query_as::<_, UserQuota>(
            r#"
            INSERT INTO user_quotas (id, user_id, daily_limit_usd, used_today_usd, reset_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (user_id) DO UPDATE
            SET updated_at = $7
            RETURNING *
            "#,
        )
        .bind(Uuid::new_v7())
        .bind(user_id)
        .bind(10000.0)
        .bind(0.0)
        .bind(reset_at)
        .bind(now)
        .bind(now)
        .fetch_one(&mut **tx)
        .await
    }

    pub async fn update_usage(
        tx: &mut Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        amount_usd: f64,
    ) -> Result<UserQuota, sqlx::Error> {
        let now = Utc::now();

        sqlx::query_as::<_, UserQuota>(
            r#"
            UPDATE user_quotas
            SET used_today_usd = used_today_usd + $1, updated_at = $2
            WHERE user_id = $3
            RETURNING *
            "#,
        )
        .bind(amount_usd)
        .bind(now)
        .bind(user_id)
        .fetch_one(&mut **tx)
        .await
    }
}

pub struct AnchorTransactionRepo;

impl AnchorTransactionRepo {
    pub async fn create(
        tx: &mut Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        route_execution_id: Uuid,
        anchor_domain: &str,
        transaction_id: &str,
        url: &str,
    ) -> Result<AnchorTransaction, sqlx::Error> {
        let id = Uuid::new_v7();
        let now = Utc::now();

        sqlx::query_as::<_, AnchorTransaction>(
            r#"
            INSERT INTO anchor_transactions
            (id, user_id, route_execution_id, anchor_domain, transaction_id, status, url, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(route_execution_id)
        .bind(anchor_domain)
        .bind(transaction_id)
        .bind("pending")
        .bind(url)
        .bind(now)
        .bind(now)
        .fetch_one(&mut **tx)
        .await
    }

    pub async fn update_status(
        tx: &mut Transaction<'_, sqlx::Postgres>,
        id: Uuid,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE anchor_transactions
            SET status = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(status)
        .bind(now)
        .bind(id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

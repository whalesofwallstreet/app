use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RouteExecution {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source_chain: String,
    pub dest_chain: String,
    pub source_asset: String,
    pub dest_asset: String,
    pub amount_in: i64,
    pub amount_out: i64,
    pub provider: String,
    pub path: String,
    pub estimated_fee_usd: f64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserHistory {
    pub id: Uuid,
    pub user_id: Uuid,
    pub route_execution_id: Uuid,
    pub action: String,
    pub details: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserQuota {
    pub id: Uuid,
    pub user_id: Uuid,
    pub daily_limit_usd: f64,
    pub used_today_usd: f64,
    pub reset_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AnchorTransaction {
    pub id: Uuid,
    pub user_id: Uuid,
    pub route_execution_id: Uuid,
    pub anchor_domain: String,
    pub transaction_id: String,
    pub status: String,
    pub url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RouteExecutionInput {
    pub user_id: Uuid,
    pub source_chain: String,
    pub dest_chain: String,
    pub source_asset: String,
    pub dest_asset: String,
    pub amount_in: i64,
    pub amount_out: i64,
    pub provider: String,
    pub path: String,
    pub estimated_fee_usd: f64,
}

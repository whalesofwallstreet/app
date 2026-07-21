use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Transaction;
use std::time::Duration;

pub mod models;
pub mod operations;
pub mod service;

#[derive(Clone)]
pub struct Database {
    pub pool: PgPool,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .connect(database_url)
            .await?;

        Ok(Database { pool })
    }

    pub async fn begin(&self) -> Result<Transaction<'static, sqlx::Postgres>, sqlx::Error> {
        self.pool.begin().await
    }

    pub async fn health_check(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }
}

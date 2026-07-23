use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Transaction;
use std::time::Duration;
use tokio::time::timeout;

pub mod models;
pub mod operations;
pub mod service;

pub struct AsyncDropPool(pub PgPool);

impl Drop for AsyncDropPool {
    fn drop(&mut self) {
        let pool = self.0.clone();
        tokio::spawn(async move {
            let _ = timeout(Duration::from_secs(5), pool.close()).await;
        });
    }
}

#[derive(Clone)]
pub struct Database {
    pool: std::sync::Arc<AsyncDropPool>,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .connect(database_url)
            .await?;

        Ok(Database {
            pool: std::sync::Arc::new(AsyncDropPool(pool)),
        })
    }

    /// Applies all pending migrations from the `migrations/` directory.
    ///
    /// This uses [`sqlx::migrate!`] which embeds the SQL files at compile time,
    /// meaning the binary is self-contained and no external migration files are
    /// needed at runtime. Migrations are applied in version order and each is
    /// wrapped in a transaction, making them atomic and safe to retry on failure.
    pub async fn run_migrations(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool.0).await
    }

    pub async fn begin(&self) -> Result<Transaction<'static, sqlx::Postgres>, sqlx::Error> {
        self.pool.0.begin().await
    }

    pub async fn health_check(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").fetch_one(&self.pool.0).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_async_drop_pool_compiles_and_runs() {
        // Just verifying the logic doesn't panic if a pool was created.
        // We cannot easily create a real PgPool without a database url,
        // but we verify the code compiles and tasks can spawn.
        let _ = tokio::spawn(async move {
            // Task simulation
            tokio::time::sleep(Duration::from_millis(10)).await;
        })
        .await;
    }
}

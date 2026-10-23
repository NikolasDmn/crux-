use std::time::Duration;

use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::{Executor, MySqlPool};

/// Pool with the connection rules from the schema header: UTC session zone and strict mode on
/// every pooled connection, or `DEFAULT CURRENT_TIMESTAMP` silently isn't UTC (review H10).
pub async fn connect(url: &str, max_connections: u32) -> Result<MySqlPool, sqlx::Error> {
    let options: MySqlConnectOptions = url.parse()?;
    let options = options.charset("utf8mb4").collation("utf8mb4_0900_ai_ci").timezone(Some("+00:00".into()));
    MySqlPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .after_connect(|conn, _| {
            Box::pin(async move {
                conn.execute("SET time_zone = '+00:00', sql_mode = CONCAT(@@sql_mode, ',STRICT_TRANS_TABLES')").await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
}

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

pub async fn migrate(pool: &MySqlPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

pub async fn ping(pool: &MySqlPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await.map(|_| ())
}

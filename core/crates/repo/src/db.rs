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

/// Migrates on a dedicated connection with slow-statement logging off (migration 0001 is ~1,000
/// lines of DDL and would otherwise be dumped into the log). Safe to call from every process at
/// start: sqlx takes a MySQL advisory lock and skips applied versions.
pub async fn migrate_url(url: &str) -> Result<(), sqlx::migrate::MigrateError> {
    use sqlx::{ConnectOptions, Connection};
    let options: MySqlConnectOptions = url.parse().map_err(sqlx::migrate::MigrateError::from)?;
    let options = options.timezone(Some("+00:00".into())).log_slow_statements(log::LevelFilter::Off, Duration::from_secs(60));
    let mut conn = sqlx::MySqlConnection::connect_with(&options).await?;
    MIGRATOR.run(&mut conn).await?;
    conn.close().await?;
    Ok(())
}

pub async fn ping(pool: &MySqlPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await.map(|_| ())
}

/// A single number from a fixed query (metrics gauges).
pub async fn scalar_i64(conn: &mut sqlx::MySqlConnection, sql: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(sql).fetch_one(conn).await
}

//! Background jobs (spec 10). Every job is idempotent, safe to run twice, and logs start/end with
//! counts.

pub mod consumer;
pub mod relay;
pub mod tokens;

use std::future::Future;
use std::time::Duration;

/// Runs `job` every `period` until `shutdown` resolves. Failures are logged and retried next tick.
pub async fn every<F, Fut>(name: &'static str, period: Duration, mut shutdown: tokio::sync::watch::Receiver<bool>, mut job: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<u64>>,
{
    let mut tick = tokio::time::interval(period);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = shutdown.changed() => break,
        }
        let started = std::time::Instant::now();
        match job().await {
            Ok(n) => tracing::info!(job = name, count = n, ms = started.elapsed().as_millis() as u64, "job done"),
            Err(e) => tracing::error!(job = name, error = %e, alert = true, "job failed"),
        }
    }
}

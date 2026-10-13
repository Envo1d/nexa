use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use tracing_appender::{non_blocking::WorkerGuard, rolling::Rotation};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub(crate) struct ShutdownLogs(pub(crate) Arc<Mutex<Option<WorkerGuard>>>);

impl ShutdownLogs {
    pub(crate) fn flush(&self) {
        if let Ok(mut guard) = self.0.lock() {
            drop(guard.take());
        }
    }
}

pub fn file_writer(
    directory: &Path,
) -> Result<(tracing_appender::non_blocking::NonBlocking, WorkerGuard), String> {
    let appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("NexaNew")
        .filename_suffix("log")
        .build(directory)
        .map_err(|error| format!("Cannot initialize log files: {error}"))?;
    // Preserve diagnostic events when the queue fills rather than silently dropping them.
    Ok(
        tracing_appender::non_blocking::NonBlockingBuilder::default()
            .lossy(false)
            .finish(appender),
    )
}

pub fn init(directory: &Path) -> Result<WorkerGuard, String> {
    let (writer, guard) = file_writer(directory)?;

    let filter = EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
        .from_env()
        .map_err(|error| format!("Invalid RUST_LOG filter: {error}"))?;
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer),
        )
        // The tracing-log feature also bridges dependency log records into tracing.
        .try_init()
        .map_err(|error| format!("Cannot initialize tracing: {error}"))?;
    Ok(guard)
}

#[cfg(test)]
mod tests;

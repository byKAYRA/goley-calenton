

use std::{fs, path::Path, sync::Mutex};

use thiserror::Error;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Debug)]
pub struct LoggingGuard;

pub fn init(path: &Path, filter: &str) -> Result<LoggingGuard, LoggingError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::try_new(filter).map_err(LoggingError::Filter)?)
        .with(
            fmt::layer()
                .json()
                .flatten_event(true)
                .with_current_span(false)
                .with_span_list(false)
                .with_writer(Mutex::new(file)),
        );
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|error| LoggingError::Subscriber(error.to_string()))?;
    Ok(LoggingGuard)
}

#[derive(Debug, Error)]
pub enum LoggingError {
    
    #[error("log file error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("invalid tracing filter: {0}")]
    Filter(tracing_subscriber::filter::ParseError),
    
    #[error("tracing subscriber error: {0}")]
    Subscriber(String),
}

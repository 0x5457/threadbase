//! Tracing sink.
//!
//! Without a log file the subscriber writes to stdout, which journald captures
//! on Linux. A configured file rotates daily and keeps a stable symlink, so
//! `threadbase service logs` never has to guess today's file name.

use anyhow::Context;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

/// Initializes the global subscriber and returns the guard that flushes the
/// file writer. The guard must live for the whole process.
pub fn init(json: bool, log_file: Option<&Path>) -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("threadbase=info,tower_http=info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    let Some(path) = log_file else {
        if json {
            builder.json().init();
        } else {
            builder.init();
        }
        return None;
    };
    match file_writer(path) {
        Ok((writer, guard)) => {
            if json {
                builder.json().with_writer(writer).init();
            } else {
                builder.with_writer(writer).init();
            }
            Some(guard)
        }
        Err(error) => {
            eprintln!("threadbase: file logging disabled ({error:#})");
            if json {
                builder.json().init();
            } else {
                builder.init();
            }
            None
        }
    }
}

fn file_writer(
    path: &Path,
) -> anyhow::Result<(tracing_appender::non_blocking::NonBlocking, WorkerGuard)> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let prefix = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("threadbase");
    let suffix = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("log");
    let mut builder = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(prefix)
        .filename_suffix(suffix)
        .max_log_files(7);
    if let Some(name) = path.file_name() {
        builder = builder.latest_symlink(name.to_string_lossy().into_owned());
    }
    let appender = builder
        .build(directory)
        .with_context(|| format!("open log directory {}", directory.display()))?;
    Ok(tracing_appender::non_blocking(appender))
}

//! One-call tracing setup that wires up the full stack:
//! - rust-debug TracingLayer (colored namespace output)
//! - tracing-error SpanTrace layer
//! - Non-blocking file appender (daily rolling)
//! - Optional JSON layer
//!
//! All controlled by the same `DEBUG` / `DEBUG_*` env vars.

use std::io;

use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use crate::TracingLayer;

/// Guard that keeps the tracing subscriber and non-blocking writer alive.
/// When dropped, flushes and shuts down.
#[must_use = "dropping this guard immediately shuts down tracing"]
pub struct TracingGuard {
    _worker_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    _file_guard: Option<crate::FileGuard>,
}

fn set_global_tracing_subscriber<S>(subscriber: S) -> io::Result<()>
where
    S: tracing::Subscriber + Send + Sync + 'static,
{
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|err| io::Error::other(err.to_string()))
}

/// Initialize the full tracing + rust-debug stack with one call.
///
/// This sets up:
/// 1. **TracingLayer** — rust-debug colored namespace output on stderr
/// 2. **tracing-error** — SpanTrace capture for panic hooks
/// 3. **File appender** — non-blocking daily-rolling log file
/// 4. **EnvFilter** — respects `RUST_LOG` for tracing-level filtering
///
/// ```rust,ignore
/// fn main() {
///     let _guard = rust_debug::init_tracing("my-app").unwrap();
///
///     rust_debug::info!("app", "started");  // colored stderr + file + tracing
///     tracing::info!(target: "app", "also works");
///
///     let _span = rust_debug::debug_span!("app:init", "loading config");
///     // ... logs -> app:init and <- app:init (Nms) on drop
/// }
/// ```
pub fn init_tracing(app_name: &str) -> io::Result<TracingGuard> {
    // Set up rust-debug's own file logging
    let file_guard = crate::init_debug_defaults(app_name)?;

    // Tracing file appender (daily rolling, non-blocking)
    let log_dir = crate::default_log_dir(app_name);
    std::fs::create_dir_all(&log_dir)?;
    let file_appender = tracing_appender::rolling::daily(log_dir, format!("{}.tracing", app_name));
    let (non_blocking, worker_guard) = tracing_appender::non_blocking(file_appender);

    // Build the env filter: RUST_LOG takes precedence, else match DEBUG level
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let default = match std::env::var("DEBUG_LEVEL")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "error" => "error",
            "warn" => "warn",
            "info" => "info",
            _ => "debug",
        };
        EnvFilter::new(default)
    });

    // Assemble the subscriber
    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(TracingLayer::all())
        .with(tracing_error::ErrorLayer::default())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true),
        );

    set_global_tracing_subscriber(registry)?;

    // Install the enhanced panic hook
    crate::install_panic_hook();

    Ok(TracingGuard {
        _worker_guard: Some(worker_guard),
        _file_guard: Some(file_guard),
    })
}

/// Initialize tracing with JSON output to a file (for machine consumption).
///
/// Same as [`init_tracing`] but the file output is JSON-formatted.
/// Requires the `json` feature.
#[cfg(feature = "json")]
pub fn init_tracing_json(app_name: &str) -> io::Result<TracingGuard> {
    let file_guard = crate::init_debug_defaults(app_name)?;

    let log_dir = crate::default_log_dir(app_name);
    std::fs::create_dir_all(&log_dir)?;
    let file_appender =
        tracing_appender::rolling::daily(log_dir, format!("{}.tracing.json", app_name));
    let (non_blocking, worker_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let default = match std::env::var("DEBUG_LEVEL")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "error" => "error",
            "warn" => "warn",
            "info" => "info",
            _ => "debug",
        };
        EnvFilter::new(default)
    });

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(TracingLayer::all())
        .with(tracing_error::ErrorLayer::default())
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_target(true)
                .with_thread_ids(true)
                .with_span_list(true),
        );

    set_global_tracing_subscriber(registry)?;
    crate::install_panic_hook();

    Ok(TracingGuard {
        _worker_guard: Some(worker_guard),
        _file_guard: Some(file_guard),
    })
}

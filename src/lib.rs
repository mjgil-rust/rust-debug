//! # rust-debug
//!
//! Namespace-based debug logging inspired by [pydebug](https://github.com/mjgil/pydebug)
//! and Node's [debug](https://www.npmjs.com/package/debug) module.
//!
//! ## Quick start
//!
//! ```bash
//! DEBUG=* ./my-app              # enable all namespaces
//! DEBUG=vnc:*,clipboard ./app   # specific namespaces
//! DEBUG=*,-key ./app            # everything except "key"
//! ```
//!
//! ```rust,ignore
//! use rust_debug::{debug, info, warn, error};
//!
//! info!("vnc", "connecting to {}:{}", host, port);
//! debug!("key", "keysym 0x{:04x}", keysym);
//! warn!("guard", "stale guard file found");
//! error!("vnc", "connection failed: {}", err);
//! ```
//!
//! ## Tracing integration
//!
//! Enable the `tracing-integration` feature to get:
//! - All rust-debug macros also emit `tracing` events
//! - `debug_span!` macro for namespace-aware spans with entry/exit logging
//! - `TracingLayer` — a tracing subscriber layer with rust-debug formatting
//! - `init_tracing()` — one-call setup for the full tracing stack
//! - `#[instrument]` re-exported from tracing
//!
//! ```rust,ignore
//! // Cargo.toml: rust-debug = { features = ["tracing-integration"] }
//! let _guard = rust_debug::init_tracing("my-app").unwrap();
//! ```
//!
//! ## Environment variables
//!
//! | Variable | Purpose | Default |
//! |---|---|---|
//! | `DEBUG` | Comma/space-separated namespace patterns | `""` (nothing) |
//! | `DEBUG_LEVEL` | Minimum level: error, warn, info, debug | `debug` |
//! | `DEBUG_COLORS` | `0` to disable colors | `1` (auto TTY) |
//! | `DEBUG_FILE` | Path to a single log file | None |
//! | `DEBUG_LOG_DIR` | Directory for daily-rotating logs | None |
//! | `DEBUG_SHOW_TIME` | Show timestamp on TTY output | `0` |
//! | `DEBUG_HIDE_DIFF` | Hide `+Nms` time differential | `0` |

use std::collections::HashMap;
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

mod formatting;
#[cfg(feature = "tracing-integration")]
mod layer;
mod span;
#[cfg(feature = "tracing-integration")]
mod tracing_init;

pub use formatting::{format_colored, format_plain, humanize, utc_timestamp};
#[cfg(feature = "tracing-integration")]
pub use layer::TracingLayer;
pub use span::SpanGuard;
#[cfg(feature = "tracing-integration")]
pub use span::{InstrumentDebug, InstrumentedFuture};
#[cfg(feature = "json")]
pub use tracing_init::init_tracing_json;
#[cfg(feature = "tracing-integration")]
pub use tracing_init::{init_tracing, TracingGuard};

// Re-export the tracing ecosystem so dependents don't need direct deps
#[cfg(feature = "tracing-integration")]
pub use tracing;
#[cfg(feature = "tracing-integration")]
pub use tracing::instrument;
#[cfg(feature = "tracing-integration")]
pub use tracing::Instrument;
#[cfg(feature = "tracing-integration")]
pub use tracing_appender;
#[cfg(feature = "tracing-integration")]
pub use tracing_core;
#[cfg(feature = "tracing-integration")]
pub use tracing_error;
#[cfg(feature = "tracing-integration")]
pub use tracing_subscriber;

use formatting::utc_date_string;

// ─── Public types ───────────────────────────────────────────────────────────

/// Log severity level. Lower numeric value = higher severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Level::Error => write!(f, "ERROR"),
            Level::Warn => write!(f, "WARN"),
            Level::Info => write!(f, "INFO"),
            Level::Debug => write!(f, "DEBUG"),
        }
    }
}

// ─── Macros ─────────────────────────────────────────────────────────────────

/// Log at ERROR level. Always shown when namespace is enabled (unless
/// `DEBUG_LEVEL` is set above error). Output is colored red on TTY.
///
/// When the `tracing-integration` feature is enabled, also emits a
/// `tracing::error!` event with the namespace as the target.
#[macro_export]
macro_rules! error {
    ($ns:expr, $($arg:tt)*) => {{
        if $crate::enabled_for($ns, $crate::Level::Error) {
            $crate::write_log($ns, $crate::Level::Error, format_args!($($arg)*));
        }
        #[cfg(feature = "tracing-integration")]
        {
            tracing::error!(target: $ns, $($arg)*);
        }
    }};
}

/// Log at WARN level. Output is colored yellow on TTY.
#[macro_export]
macro_rules! warn {
    ($ns:expr, $($arg:tt)*) => {{
        if $crate::enabled_for($ns, $crate::Level::Warn) {
            $crate::write_log($ns, $crate::Level::Warn, format_args!($($arg)*));
        }
        #[cfg(feature = "tracing-integration")]
        {
            tracing::warn!(target: $ns, $($arg)*);
        }
    }};
}

/// Log at INFO level. Output uses the namespace's assigned color on TTY.
#[macro_export]
macro_rules! info {
    ($ns:expr, $($arg:tt)*) => {{
        if $crate::enabled_for($ns, $crate::Level::Info) {
            $crate::write_log($ns, $crate::Level::Info, format_args!($($arg)*));
        }
        #[cfg(feature = "tracing-integration")]
        {
            tracing::info!(target: $ns, $($arg)*);
        }
    }};
}

/// Log at DEBUG level. Most verbose. Output uses the namespace's assigned
/// color on TTY.
#[macro_export]
macro_rules! debug {
    ($ns:expr, $($arg:tt)*) => {{
        if $crate::enabled_for($ns, $crate::Level::Debug) {
            $crate::write_log($ns, $crate::Level::Debug, format_args!($($arg)*));
        }
        #[cfg(feature = "tracing-integration")]
        {
            tracing::debug!(target: $ns, $($arg)*);
        }
    }};
}

/// Create a namespace-aware span. Returns a guard that logs entry and exit
/// with duration via rust-debug, and (when tracing is enabled) also creates
/// a proper tracing span.
///
/// # Without tracing feature
/// ```rust,ignore
/// let _guard = rust_debug::debug_span!("vnc:render", "frame {}", frame_num);
/// // logs: vnc:render -> frame 42
/// // on drop: vnc:render <- frame 42 (3.2ms)
/// ```
///
/// # With tracing feature
/// Also creates a `tracing::info_span!` that integrates with the full tracing
/// ecosystem (subscribers, OpenTelemetry, tokio-console, etc.).
#[macro_export]
macro_rules! debug_span {
    ($ns:expr, $($arg:tt)*) => {{
        let _msg = format!($($arg)*);
        $crate::SpanGuard::new($ns, &_msg)
    }};
}

// ─── Global state ───────────────────────────────────────────────────────────

static STATE: OnceLock<GlobalState> = OnceLock::new();

struct GlobalState {
    includes: Vec<String>,
    excludes: Vec<String>,
    min_level: Level,
    use_colors: bool,
    show_time: bool,
    show_diff: bool,
    stderr_is_tty: bool,
    /// When true, all namespaces are enabled regardless of DEBUG env var,
    /// unless explicitly excluded. Set by `init_debug_defaults()`.
    all_enabled: AtomicBool,
    namespaces: Mutex<HashMap<String, NamespaceState>>,
    file_writer: Mutex<Option<FileWriter>>,
    color_index: Mutex<usize>,
    session_marker_emitted: AtomicBool,
}

struct NamespaceState {
    enabled: bool,
    color: u8,
    last_call: Option<Instant>,
}

enum FileWriter {
    Single(std::fs::File),
    Rolling {
        dir: PathBuf,
        prefix: String,
        current_date: String,
        file: std::fs::File,
    },
}

/// ANSI color indices cycled per namespace (same as pydebug):
/// cyan, green, yellow, blue, magenta, red
pub const COLORS: [u8; 6] = [6, 2, 3, 4, 5, 1];

fn state() -> &'static GlobalState {
    STATE.get_or_init(|| {
        let debug_env = std::env::var("DEBUG").unwrap_or_default();
        let (includes, excludes) = parse_debug_patterns(&debug_env);

        let min_level = match std::env::var("DEBUG_LEVEL")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "error" => Level::Error,
            "warn" => Level::Warn,
            "info" => Level::Info,
            _ => Level::Debug,
        };

        let use_colors = std::env::var("DEBUG_COLORS")
            .map(|v| v != "0")
            .unwrap_or(true);

        let show_time = std::env::var("DEBUG_SHOW_TIME")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let show_diff = !std::env::var("DEBUG_HIDE_DIFF")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let stderr_is_tty = io::stderr().is_terminal();

        // Auto-init file writer from env vars
        let file_writer = init_file_writer_from_env();

        let has_includes = !includes.is_empty();

        let s = GlobalState {
            includes,
            excludes,
            min_level,
            use_colors,
            show_time,
            show_diff,
            stderr_is_tty,
            all_enabled: AtomicBool::new(false),
            namespaces: Mutex::new(HashMap::new()),
            file_writer: Mutex::new(file_writer),
            color_index: Mutex::new(0),
            session_marker_emitted: AtomicBool::new(false),
        };

        // Session start marker (only if something is enabled)
        if has_includes {
            emit_session_marker(&s);
        }

        s
    })
}

/// Emit session marker exactly once.
fn emit_session_marker(s: &GlobalState) {
    if s.session_marker_emitted
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return;
    }

    let pid = std::process::id();
    let ts = utc_timestamp();
    let marker = format!("=== session started: {} pid={} ===\n", ts, pid);
    let _ = io::stderr().write_all(marker.as_bytes());
    if let Ok(mut fw) = s.file_writer.lock() {
        if let Some(ref mut w) = *fw {
            let _ = file_write(w, &marker);
        }
    }
}

fn init_file_writer_from_env() -> Option<FileWriter> {
    if let Ok(path) = std::env::var("DEBUG_FILE") {
        return std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
            .map(FileWriter::Single);
    }
    if let Ok(dir) = std::env::var("DEBUG_LOG_DIR") {
        let _ = std::fs::create_dir_all(&dir);
        let date = utc_date_string();
        let prefix = "debug".to_string();
        let path = PathBuf::from(&dir).join(format!("{}.{}.log", prefix, date));
        return std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
            .map(|file| FileWriter::Rolling {
                dir: PathBuf::from(dir),
                prefix,
                current_date: date,
                file,
            });
    }
    None
}

// ─── Pattern parsing & matching ─────────────────────────────────────────────

pub fn parse_debug_patterns(env: &str) -> (Vec<String>, Vec<String>) {
    let mut includes = Vec::new();
    let mut excludes = Vec::new();

    for part in env.split(|c: char| c == ',' || c.is_whitespace()) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(rest) = part.strip_prefix('-') {
            excludes.push(rest.to_string());
        } else {
            includes.push(part.to_string());
        }
    }

    (includes, excludes)
}

pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    pattern == name
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Check if a namespace is enabled at the given level.
pub fn enabled_for(namespace: &str, level: Level) -> bool {
    let s = state();
    if level > s.min_level {
        return false;
    }
    namespace_enabled(s, namespace)
}

/// Check if a namespace is enabled (at any level).
pub fn enabled(namespace: &str) -> bool {
    namespace_enabled(state(), namespace)
}

fn namespace_enabled(s: &GlobalState, namespace: &str) -> bool {
    let force = s.all_enabled.load(Ordering::Relaxed);
    let mut namespaces = s.namespaces.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(ns) = namespaces.get(namespace) {
        return ns.enabled || (force && !namespace_excluded(s, namespace));
    }

    let enabled = namespace_allowed(s, namespace, force);
    let color = next_namespace_color(s);
    namespaces.insert(
        namespace.to_string(),
        NamespaceState {
            enabled,
            color,
            last_call: None,
        },
    );

    enabled
}

fn namespace_allowed(s: &GlobalState, namespace: &str, force: bool) -> bool {
    !namespace_excluded(s, namespace)
        && (force || s.includes.iter().any(|p| pattern_matches(p, namespace)))
}

fn namespace_excluded(s: &GlobalState, namespace: &str) -> bool {
    s.excludes.iter().any(|p| pattern_matches(p, namespace))
}

fn next_namespace_color(s: &GlobalState) -> u8 {
    let mut idx = s.color_index.lock().unwrap_or_else(|e| e.into_inner());
    let color = COLORS[*idx % COLORS.len()];
    *idx += 1;
    color
}

/// Write a log message. Called by macros — not for direct use.
#[doc(hidden)]
pub fn write_log(namespace: &str, level: Level, args: fmt::Arguments<'_>) {
    let s = state();

    // Update namespace state and get color + time diff
    let (color, diff_us) = {
        let mut namespaces = s.namespaces.lock().unwrap_or_else(|e| e.into_inner());
        let ns = match namespaces.get_mut(namespace) {
            Some(ns) => ns,
            None => return, // shouldn't happen — enabled_for registers first
        };
        let now = Instant::now();
        let diff = ns
            .last_call
            .map(|t| now.duration_since(t).as_micros())
            .unwrap_or(0);
        ns.last_call = Some(now);
        (ns.color, diff)
    };
    // Mutex released — safe to do I/O now

    let diff_str = if s.show_diff {
        humanize(diff_us)
    } else {
        String::new()
    };

    // Capture timestamp once for both outputs
    let ts = utc_timestamp();

    // stderr
    let stderr_line = if s.stderr_is_tty && s.use_colors {
        format_colored(
            namespace,
            level,
            color,
            &args,
            &diff_str,
            s.show_time,
            s.show_diff,
            &ts,
        )
    } else {
        format_plain(namespace, level, &args, &diff_str, s.show_diff, &ts)
    };
    let _ = io::stderr().write_all(stderr_line.as_bytes());

    // file writer
    if let Ok(mut fw) = s.file_writer.lock() {
        if let Some(ref mut w) = *fw {
            let file_line = format_plain(namespace, level, &args, &diff_str, s.show_diff, &ts);
            let _ = file_write(w, &file_line);
        }
    }
}

// ─── File logging ───────────────────────────────────────────────────────────

/// Guard returned by [`init_file_logger`] and [`init_rolling_logger`].
/// The file writer is flushed and removed when this guard is dropped.
#[must_use = "if the guard is dropped immediately, the file writer is removed"]
pub struct FileGuard {
    _private: (),
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        if let Some(s) = STATE.get() {
            if let Ok(mut fw) = s.file_writer.lock() {
                if let Some(ref mut w) = *fw {
                    let _ = flush_file_writer(w);
                }
                *fw = None;
            }
        }
    }
}

/// Initialize file logging to a single file.
pub fn init_file_logger(path: &str) -> io::Result<FileGuard> {
    let s = state();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut fw = s.file_writer.lock().unwrap_or_else(|e| e.into_inner());
    *fw = Some(FileWriter::Single(file));
    Ok(FileGuard { _private: () })
}

/// Initialize daily-rotating file logging.
pub fn init_rolling_logger(dir: &str, prefix: &str) -> io::Result<FileGuard> {
    let s = state();
    std::fs::create_dir_all(dir)?;
    let date = utc_date_string();
    let path = PathBuf::from(dir).join(format!("{}.{}.log", prefix, date));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let mut fw = s.file_writer.lock().unwrap_or_else(|e| e.into_inner());
    *fw = Some(FileWriter::Rolling {
        dir: PathBuf::from(dir),
        prefix: prefix.to_string(),
        current_date: date,
        file,
    });
    Ok(FileGuard { _private: () })
}

/// Enable all namespaces and set up rolling file logging.
///
/// Intended for debug builds — call early in `main()`:
/// ```rust,ignore
/// #[cfg(debug_assertions)]
/// let _log = rust_debug::init_debug_defaults("my-app");
/// ```
///
/// Log files go to a platform-appropriate state directory:
/// - Linux: `$XDG_STATE_HOME/{app_name}` or `~/.local/state/{app_name}`
/// - macOS: `~/Library/Logs/{app_name}`
///
/// In release builds, use `DEBUG` env var instead (no code change needed).
pub fn init_debug_defaults(app_name: &str) -> io::Result<FileGuard> {
    let s = state();
    s.all_enabled.store(true, Ordering::Relaxed);

    // Emit session marker (idempotent — skips if already emitted during state init)
    emit_session_marker(s);

    let dir = default_log_dir(app_name);
    std::fs::create_dir_all(&dir)?;

    let guard = init_rolling_logger(&dir.to_string_lossy(), app_name)?;

    // Write session marker to the file if it was just opened
    let pid = std::process::id();
    let ts = utc_timestamp();
    let marker = format!("=== session started: {} pid={} ===\n", ts, pid);
    if let Ok(mut fw) = s.file_writer.lock() {
        if let Some(ref mut w) = *fw {
            let _ = file_write(w, &marker);
        }
    }

    Ok(guard)
}

/// Platform-appropriate log directory for an application.
///
/// - Linux: `$XDG_STATE_HOME/{app_name}` or `~/.local/state/{app_name}`
/// - macOS: `~/Library/Logs/{app_name}`
/// - Fallback: `./{app_name}`
pub fn default_log_dir(app_name: &str) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Logs").join(app_name);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(state) = std::env::var("XDG_STATE_HOME") {
            return PathBuf::from(state).join(app_name);
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("state")
                .join(app_name);
        }
    }

    PathBuf::from(".").join(app_name)
}

/// Install a panic hook that logs the panic to stderr and file, then flushes.
///
/// When `tracing-integration` is enabled, also captures a [`SpanTrace`] showing
/// the active span stack at the time of the panic.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let ts = utc_timestamp();

        #[cfg(feature = "tracing-integration")]
        {
            let span_trace = tracing_error::SpanTrace::capture();
            let line = format!("{} PANIC {}\n  span trace:\n{}\n", ts, info, span_trace);
            let _ = io::stderr().write_all(line.as_bytes());
            let _ = io::stderr().flush();

            if let Some(s) = STATE.get() {
                if let Ok(mut fw) = s.file_writer.lock() {
                    if let Some(ref mut w) = *fw {
                        let _ = file_write(w, &line);
                        let _ = flush_file_writer(w);
                    }
                }
            }
        }

        #[cfg(not(feature = "tracing-integration"))]
        {
            let line = format!("{} PANIC {}\n", ts, info);
            let _ = io::stderr().write_all(line.as_bytes());
            let _ = io::stderr().flush();

            if let Some(s) = STATE.get() {
                if let Ok(mut fw) = s.file_writer.lock() {
                    if let Some(ref mut w) = *fw {
                        let _ = file_write(w, &line);
                        let _ = flush_file_writer(w);
                    }
                }
            }
        }
    }));
}

// ─── File I/O ───────────────────────────────────────────────────────────────

fn file_write(writer: &mut FileWriter, msg: &str) -> io::Result<()> {
    match writer {
        FileWriter::Single(file) => {
            file.write_all(msg.as_bytes())?;
            file.flush()
        }
        FileWriter::Rolling {
            dir,
            prefix,
            current_date,
            file,
        } => {
            let date = utc_date_string();
            if date != *current_date {
                let path = dir.join(format!("{}.{}.log", prefix, date));
                *file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)?;
                *current_date = date;
            }
            file.write_all(msg.as_bytes())?;
            file.flush()
        }
    }
}

fn flush_file_writer(writer: &mut FileWriter) -> io::Result<()> {
    match writer {
        FileWriter::Single(f) => f.flush(),
        FileWriter::Rolling { file, .. } => file.flush(),
    }
}

#[cfg(test)]
mod tests;

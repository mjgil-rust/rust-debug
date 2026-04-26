# API Reference

## Macros

### `debug!(namespace, fmt, args...)`

Log at DEBUG level. Most verbose. Output uses the namespace's assigned color on TTY.

```rust
rust_debug::debug!("vnc", "keysym 0x{:04x}", keysym);
```

### `info!(namespace, fmt, args...)`

Log at INFO level. Output uses the namespace's assigned color on TTY.

```rust
rust_debug::info!("vnc", "connecting to {}:{}", host, port);
```

### `warn!(namespace, fmt, args...)`

Log at WARN level. Output is colored yellow on TTY.

```rust
rust_debug::warn!("guard", "stale guard file found");
```

### `error!(namespace, fmt, args...)`

Log at ERROR level. Always shown when namespace is enabled (unless `DEBUG_LEVEL` is set above error). Output is colored red on TTY.

```rust
rust_debug::error!("vnc", "connection failed: {}", err);
```

### `debug_span!(namespace, fmt, args...)`

Create a namespace-aware span guard. Logs `-> message` on creation and `<- message (duration)` on drop.

```rust
let _guard = rust_debug::debug_span!("vnc:render", "frame {}", frame_num);
// logs: vnc:render -> frame 42
// on drop: vnc:render <- frame 42 (3.2ms)
```

When the `tracing-integration` feature is enabled, also creates a `tracing::info_span!`.

## Functions

### `enabled(namespace) -> bool`

Check if a namespace is enabled at any level.

### `enabled_for(namespace, level) -> bool`

Check if a namespace is enabled at a specific `Level`.

### `init_file_logger(path) -> io::Result<FileGuard>`

Initialize file logging to a single file. Returns a guard — logging stops when the guard is dropped.

```rust
let _guard = rust_debug::init_file_logger("/tmp/app.log")?;
```

### `init_rolling_logger(dir, prefix) -> io::Result<FileGuard>`

Initialize daily-rotating file logging. Files are named `{prefix}.{YYYY-MM-DD}.log`.

```rust
let _guard = rust_debug::init_rolling_logger("/var/log/myapp", "myapp")?;
```

### `init_debug_defaults(app_name) -> io::Result<FileGuard>`

Enable all namespaces and set up rolling file logging in a platform-appropriate directory. Intended for debug builds.

```rust
#[cfg(debug_assertions)]
let _log = rust_debug::init_debug_defaults("my-app");
```

Log directory locations:
- Linux: `$XDG_STATE_HOME/{app_name}` or `~/.local/state/{app_name}`
- macOS: `~/Library/Logs/{app_name}`

### `default_log_dir(app_name) -> PathBuf`

Returns the platform-appropriate log directory path without creating it.

### `install_panic_hook()`

Install a panic hook that logs panics to stderr and file, then flushes. With `tracing-integration`, also captures a `SpanTrace`.

## Types

### `Level`

Log severity level. Lower numeric value = higher severity.

```rust
pub enum Level {
    Error = 0,
    Warn  = 1,
    Info  = 2,
    Debug = 3,
}
```

### `FileGuard`

RAII guard returned by file logging initializers. The file writer is flushed and removed when dropped.

### `SpanGuard`

RAII guard returned by `debug_span!`. Logs entry on creation and exit with duration on drop.

## Tracing Feature (`tracing-integration`)

### `TracingLayer`

A `tracing_subscriber::Layer` that formats events using rust-debug's colored, namespace-based output.

```rust
use tracing_subscriber::prelude::*;

// From DEBUG env var
tracing_subscriber::registry()
    .with(rust_debug::TracingLayer::from_env())
    .init();

// All namespaces enabled
tracing_subscriber::registry()
    .with(rust_debug::TracingLayer::all())
    .init();
```

Builder methods:
- `.with_colors(bool)` — override color output
- `.with_diff(bool)` — override time differential display
- `.with_time(bool)` — override timestamp display

### `init_tracing(app_name) -> io::Result<TracingGuard>`

One-call setup for the full tracing + rust-debug stack:
1. `TracingLayer` — colored namespace output on stderr
2. `tracing-error` — SpanTrace capture for panic hooks
3. File appender — non-blocking daily-rolling log file
4. `EnvFilter` — respects `RUST_LOG` for tracing-level filtering

```rust
let _guard = rust_debug::init_tracing("my-app").unwrap();
```

### `init_tracing_json(app_name) -> io::Result<TracingGuard>`

Same as `init_tracing` but file output is JSON-formatted. Requires the `json` feature.

### `TracingGuard`

RAII guard that keeps the tracing subscriber and non-blocking writer alive. Must be held for the program's lifetime.

### `InstrumentDebug` trait

Extension trait for attaching a namespace-aware span to a future.

```rust
use rust_debug::InstrumentDebug;

async move { do_work().await }
    .instrument_debug("vnc:save", "saving document")
    .await;
```

## Re-exports (with `tracing-integration`)

- `tracing` — the tracing crate
- `tracing_core`
- `tracing_subscriber`
- `tracing_appender`
- `tracing_error`
- `instrument` — `#[instrument]` attribute macro
- `Instrument` — `.instrument()` trait

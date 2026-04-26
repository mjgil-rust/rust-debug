# rust-debug

Namespace-based debug logging inspired by [pydebug](https://github.com/mjgil/pydebug) and Node's [debug](https://www.npmjs.com/package/debug) module.

## Quick start

```bash
DEBUG=* ./my-app              # enable all namespaces
DEBUG=vnc:*,clipboard ./app   # specific namespaces
DEBUG=*,-key ./app            # everything except "key"
```

```rust
use rust_debug::{debug, info, warn, error};

info!("vnc", "connecting to {}:{}", host, port);
debug!("key", "keysym 0x{:04x}", keysym);
warn!("guard", "stale guard file found");
error!("vnc", "connection failed: {}", err);
```

## Environment variables

| Variable | Purpose | Default |
|---|---|---|
| `DEBUG` | Comma/space-separated namespace patterns | `""` (nothing) |
| `DEBUG_LEVEL` | Minimum level: error, warn, info, debug | `debug` |
| `DEBUG_COLORS` | `0` to disable colors | `1` (auto TTY) |
| `DEBUG_FILE` | Path to a single log file | None |
| `DEBUG_LOG_DIR` | Directory for daily-rotating logs | None |
| `DEBUG_SHOW_TIME` | Show timestamp on TTY output | `0` |
| `DEBUG_HIDE_DIFF` | Hide `+Nms` time differential | `0` |

## Features

| Feature | Description |
|---|---|
| *(default)* | Standalone macro-based logging, no dependencies |
| `tracing-integration` | Emit tracing events, `debug_span!`, `TracingLayer`, `init_tracing()` |
| `json` | JSON-formatted tracing output |
| `full` | All of the above |

## Tracing integration

Enable the `tracing-integration` feature to get:

- All rust-debug macros also emit `tracing` events
- `debug_span!` macro for namespace-aware spans with entry/exit logging
- `TracingLayer` — a tracing subscriber layer with rust-debug formatting
- `init_tracing()` — one-call setup for the full tracing stack
- `#[instrument]` re-exported from tracing

```rust
// Cargo.toml: rust-debug = { features = ["tracing-integration"] }
let _guard = rust_debug::init_tracing("my-app").unwrap();
```

## Default logging with `init_debug_defaults`

For applications that should always log (not just in debug builds), call `init_debug_defaults` unconditionally in `main()`. This enables all namespaces and sets up daily-rotating log files in a platform-appropriate directory:

```rust
fn main() {
    let _log_guard = rust_debug::init_debug_defaults("my-app");
    // Logs to ~/.local/state/my-app/ (Linux) or ~/Library/Logs/my-app/ (macOS)
}
```

No environment variables are needed — logging is always on. You can still use `DEBUG` exclusion patterns (e.g. `DEBUG=-noisy:*`) to suppress specific namespaces.

## License

This project is licensed under the ISC License. See [LICENSE](LICENSE) for details.
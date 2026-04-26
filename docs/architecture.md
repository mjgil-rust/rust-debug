# Architecture

## Overview

rust-debug is a namespace-based debug logging library with an optional tracing integration layer. It follows the same mental model as Node's [debug](https://www.npmjs.com/package/debug) and Python's [pydebug](https://github.com/mjgil/pydebug): logging is controlled by pattern-matching namespace strings against the `DEBUG` environment variable.

## Module Structure

```
src/
├── lib.rs            # Core: macros, global state, pattern matching, configuration, file I/O
├── formatting.rs     # Shared formatting and UTC timestamp utilities
├── span.rs           # SpanGuard and InstrumentDebug helpers shared by debug_span!/tracing
├── layer.rs          # TracingLayer: tracing subscriber layer (feature-gated)
└── tracing_init.rs   # init_tracing(): one-call tracing stack setup (feature-gated)
```

## Core Design (`lib.rs`)

### Global State

All logging state lives in a single `GlobalState` struct behind a `OnceLock`. It is initialized lazily on first use by reading environment variables. This means configuration is immutable after the first log call, with the exception of `all_enabled` (an `AtomicBool` toggled by `init_debug_defaults`).

```
GlobalState
├── includes / excludes    — parsed from DEBUG env var
├── min_level              — from DEBUG_LEVEL
├── use_colors             — from DEBUG_COLORS
├── show_time / show_diff  — from DEBUG_SHOW_TIME / DEBUG_HIDE_DIFF
├── namespaces             — Mutex<HashMap> of per-namespace state (enabled, color, last_call)
├── file_writer            — Mutex<Option<FileWriter>> for file output
├── color_index            — Mutex<usize> cycling through COLORS
└── all_enabled            — AtomicBool, set by init_debug_defaults()
```

### Namespace Resolution

When a namespace is first seen (via `enabled_for`):
1. Check if it's excluded by any pattern in `excludes`
2. Check if it's included by any pattern in `includes`, or if `all_enabled` is set
3. Assign it the next color from the `COLORS` array (cycling through cyan, green, yellow, blue, magenta, red)
4. Cache the result in the `namespaces` HashMap

Subsequent calls for the same namespace hit the cache directly.

### Pattern Matching

Patterns support three forms:
- `*` — matches everything
- `prefix*` — matches any namespace starting with `prefix`
- `exact` — matches only that exact string

Patterns prefixed with `-` in the `DEBUG` env var become exclusions. Exclusions always take priority over inclusions.

### Macro Flow

Each logging macro (`debug!`, `info!`, `warn!`, `error!`) does:
1. Call `enabled_for(namespace, level)` — returns early if disabled
2. Call `write_log(namespace, level, formatted_args)`
3. If `tracing-integration` is enabled, also emit a `tracing::` event

`write_log` acquires the namespace mutex to get the color and compute the time differential since the last call for that namespace, then releases the mutex before doing any I/O.

### Output

All output goes to stderr. Optionally, output also goes to a file writer:
- **Single file** — append mode, set via `DEBUG_FILE` or `init_file_logger()`
- **Rolling file** — daily rotation by date string, set via `DEBUG_LOG_DIR` or `init_rolling_logger()`

Colored output uses ANSI escape codes and is only enabled when stderr is a TTY and `DEBUG_COLORS` is not `0`.

### Span Guards (`span.rs`)

`debug_span!` returns a `SpanGuard` that:
- Logs `-> message` on creation
- Logs `<- message (duration)` on drop
- With `tracing-integration`, also holds an `EnteredSpan` from tracing carrying `ns` / `msg` fields alongside the rust-debug marker

The tracing span created for `debug_span!` / `InstrumentDebug` carries a private `rust_debug_managed=true` marker so `TracingLayer` can recognize that rust-debug already emitted the human-readable entry/exit lines and avoid rendering a duplicate copy.

## Tracing Layer (`layer.rs`)

`TracingLayer` implements `tracing_subscriber::Layer` and provides an independent rendering pipeline that formats tracing events using rust-debug's style. It maintains its own namespace state (separate from `GlobalState`) so it can be used standalone or alongside the core macros.

Key behavior:
- **`on_event`** — extracts namespace from the `ns`/`namespace` field or falls back to `target`, collects structured fields, formats with rust-debug colors
- **`on_new_span`** — stores `SpanTiming` data in span extensions for user-created tracing spans, but skips rust-debug-managed spans marked with `rust_debug_managed=true`
- **`on_enter`** / **`on_exit`** — logs `->` / `<-` with duration using span extension data

## Tracing Init (`tracing_init.rs`)

`init_tracing()` assembles a full subscriber stack:

```
tracing_subscriber::registry()
├── EnvFilter (from RUST_LOG or DEBUG_LEVEL)
├── TracingLayer::all() (stderr, colored)
├── tracing_error::ErrorLayer (SpanTrace capture)
└── fmt::layer (non-blocking daily-rolling file appender)
```

It also calls `init_debug_defaults()` to enable rust-debug's own file logging and `install_panic_hook()` for enhanced panic output.

Unlike the earlier implementation, subscriber installation uses `tracing::subscriber::set_global_default` and returns an `io::Error` if another global subscriber is already installed. File logging initialization errors are also propagated instead of being dropped.

## Feature Flags

| Feature | Adds | Dependencies |
|---|---|---|
| *(default)* | Core macros and file logging | None |
| `tracing-integration` | `TracingLayer`, `init_tracing()`, `debug_span!` with tracing, `InstrumentDebug`, re-exports | tracing, tracing-core, tracing-subscriber, tracing-appender, tracing-error |
| `json` | `init_tracing_json()` with JSON file output | tracing-subscriber/json |
| `full` | Everything | All of the above |

The zero-dependency default keeps the library lightweight for projects that only need the `DEBUG` env var pattern without the tracing ecosystem.

## Thread Safety

- `GlobalState` is behind `OnceLock` (initialized once, read-only after)
- `namespaces`, `file_writer`, and `color_index` are each in their own `Mutex`
- The namespace mutex is released before any I/O to avoid holding it during slow operations
- `all_enabled` and `session_marker_emitted` use `AtomicBool` for lock-free access
- `TracingLayer` has its own independent mutex-protected state

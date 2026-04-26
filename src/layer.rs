//! Tracing subscriber layer that renders events and spans using rust-debug's
//! namespace-colored, time-differential formatting.
//!
//! Enable with the `tracing-integration` feature flag.

use std::collections::HashMap;
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::Mutex;
use std::time::Instant;

use tracing_core::field::{Field, Visit};
use tracing_core::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::{parse_debug_patterns, pattern_matches, COLORS};

// ─── Namespace state ─────────────────────────────────────────────────────────

struct NamespaceState {
    color: u8,
    last_call: Option<Instant>,
}

// ─── TracingLayer ────────────────────────────────────────────────────────────

/// A [`tracing_subscriber::Layer`] that formats events using rust-debug's
/// colored, namespace-based output with time differentials.
///
/// # Usage
///
/// ```rust,ignore
/// use tracing_subscriber::prelude::*;
/// use rust_debug::TracingLayer;
///
/// tracing_subscriber::registry()
///     .with(TracingLayer::from_env())
///     .init();
/// ```
pub struct TracingLayer {
    includes: Vec<String>,
    excludes: Vec<String>,
    namespaces: Mutex<HashMap<String, NamespaceState>>,
    color_index: Mutex<usize>,
    use_colors: bool,
    show_diff: bool,
    show_time: bool,
    stderr_is_tty: bool,
}

impl TracingLayer {
    /// Create a new layer reading `DEBUG`, `DEBUG_COLORS`, `DEBUG_SHOW_TIME`,
    /// `DEBUG_HIDE_DIFF` from the environment.
    pub fn from_env() -> Self {
        let debug_env = std::env::var("DEBUG").unwrap_or_default();
        let (includes, excludes) = parse_debug_patterns(&debug_env);
        let stderr_is_tty = io::stderr().is_terminal();

        let use_colors = std::env::var("DEBUG_COLORS")
            .map(|v| v != "0")
            .unwrap_or(true)
            && stderr_is_tty;

        let show_time = std::env::var("DEBUG_SHOW_TIME")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let show_diff = !std::env::var("DEBUG_HIDE_DIFF")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        Self {
            includes,
            excludes,
            namespaces: Mutex::new(HashMap::new()),
            color_index: Mutex::new(0),
            use_colors,
            show_diff,
            show_time,
            stderr_is_tty,
        }
    }

    /// Create a layer that shows all namespaces (equivalent to `DEBUG=*`).
    pub fn all() -> Self {
        let mut layer = Self::from_env();
        layer.includes = vec!["*".to_string()];
        layer
    }

    /// Override whether to show colors.
    pub fn with_colors(mut self, colors: bool) -> Self {
        self.use_colors = colors;
        self
    }

    /// Override whether to show time differentials.
    pub fn with_diff(mut self, diff: bool) -> Self {
        self.show_diff = diff;
        self
    }

    /// Override whether to show timestamps.
    pub fn with_time(mut self, time: bool) -> Self {
        self.show_time = time;
        self
    }

    fn is_enabled(&self, namespace: &str) -> bool {
        let excluded = self.excludes.iter().any(|p| pattern_matches(p, namespace));
        if excluded {
            return false;
        }
        self.includes.iter().any(|p| pattern_matches(p, namespace))
    }

    fn get_color_and_diff(&self, namespace: &str) -> (u8, u128) {
        let mut namespaces = self.namespaces.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();

        if let Some(ns) = namespaces.get_mut(namespace) {
            let diff = ns
                .last_call
                .map(|t| now.duration_since(t).as_micros())
                .unwrap_or(0);
            ns.last_call = Some(now);
            (ns.color, diff)
        } else {
            let color = {
                let mut idx = self.color_index.lock().unwrap_or_else(|e| e.into_inner());
                let c = COLORS[*idx % COLORS.len()];
                *idx += 1;
                c
            };
            namespaces.insert(
                namespace.to_string(),
                NamespaceState {
                    color,
                    last_call: Some(now),
                },
            );
            (color, 0)
        }
    }
}

// ─── Field visitor ───────────────────────────────────────────────────────────

struct FieldCollector {
    message: String,
    fields: Vec<(String, String)>,
    namespace: Option<String>,
}

impl FieldCollector {
    fn new() -> Self {
        Self {
            message: String::new(),
            fields: Vec::new(),
            namespace: None,
        }
    }
}

#[derive(Default)]
struct SpanFieldValues {
    namespace: Option<String>,
    message: Option<String>,
    rust_debug_managed: bool,
}

struct SpanFieldCollector {
    values: SpanFieldValues,
}

impl SpanFieldCollector {
    fn new() -> Self {
        Self {
            values: SpanFieldValues::default(),
        }
    }

    fn into_values(self) -> SpanFieldValues {
        self.values
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SpanDescriptor {
    namespace: String,
    name: String,
}

fn normalize_debug_value(value: &dyn fmt::Debug) -> String {
    let value = format!("{:?}", value);
    value
        .strip_prefix('"')
        .and_then(|trimmed| trimmed.strip_suffix('"'))
        .unwrap_or(&value)
        .to_string()
}

fn resolve_span_descriptor(
    target: &str,
    default_name: &str,
    fields: SpanFieldValues,
) -> Option<SpanDescriptor> {
    if fields.rust_debug_managed {
        return None;
    }

    Some(SpanDescriptor {
        namespace: fields.namespace.unwrap_or_else(|| target.to_string()),
        name: fields.message.unwrap_or_else(|| default_name.to_string()),
    })
}

impl Visit for FieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else if field.name() == "namespace" || field.name() == "ns" {
            self.namespace = Some(format!("{:?}", value));
        } else {
            self.fields
                .push((field.name().to_string(), format!("{:?}", value)));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else if field.name() == "namespace" || field.name() == "ns" {
            self.namespace = Some(value.to_string());
        } else {
            self.fields
                .push((field.name().to_string(), value.to_string()));
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }
}

impl Visit for SpanFieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        match field.name() {
            "namespace" | "ns" => self.values.namespace = Some(normalize_debug_value(value)),
            "message" | "msg" => self.values.message = Some(normalize_debug_value(value)),
            _ => {}
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "namespace" | "ns" => self.values.namespace = Some(value.to_string()),
            "message" | "msg" => self.values.message = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        if field.name() == "rust_debug_managed" {
            self.values.rust_debug_managed = value;
        }
    }
}

// ─── Layer implementation ────────────────────────────────────────────────────

impl<S> Layer<S> for TracingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut collector = FieldCollector::new();
        event.record(&mut collector);

        // Determine namespace: explicit field > target
        let namespace = collector
            .namespace
            .unwrap_or_else(|| event.metadata().target().to_string());

        if !self.is_enabled(&namespace) {
            return;
        }

        let level = match *event.metadata().level() {
            tracing_core::Level::ERROR => crate::Level::Error,
            tracing_core::Level::WARN => crate::Level::Warn,
            tracing_core::Level::INFO => crate::Level::Info,
            _ => crate::Level::Debug,
        };

        let (color, diff_us) = self.get_color_and_diff(&namespace);

        // Build message with structured fields appended
        let mut msg = collector.message;
        if !collector.fields.is_empty() {
            if !msg.is_empty() {
                msg.push(' ');
            }
            let field_strs: Vec<String> = collector
                .fields
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            msg.push_str(&field_strs.join(" "));
        }

        let diff_str = if self.show_diff {
            crate::humanize(diff_us)
        } else {
            String::new()
        };

        let ts = crate::utc_timestamp();

        let line = if self.stderr_is_tty && self.use_colors {
            crate::format_colored(
                &namespace,
                level,
                color,
                &format_args!("{}", msg),
                &diff_str,
                self.show_time,
                self.show_diff,
                &ts,
            )
        } else {
            crate::format_plain(
                &namespace,
                level,
                &format_args!("{}", msg),
                &diff_str,
                self.show_diff,
                &ts,
            )
        };
        let _ = io::stderr().write_all(line.as_bytes());
    }

    fn on_new_span(
        &self,
        attrs: &tracing_core::span::Attributes<'_>,
        id: &tracing_core::span::Id,
        ctx: Context<'_, S>,
    ) {
        let mut collector = SpanFieldCollector::new();
        attrs.record(&mut collector);

        let Some(span_descriptor) = resolve_span_descriptor(
            attrs.metadata().target(),
            attrs.metadata().name(),
            collector.into_values(),
        ) else {
            return;
        };

        if !self.is_enabled(&span_descriptor.namespace) {
            return;
        }

        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanTiming {
                entered: None,
                namespace: span_descriptor.namespace,
                name: span_descriptor.name,
            });
        }
    }

    fn on_enter(&self, id: &tracing_core::span::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut ext = span.extensions_mut();
            if let Some(timing) = ext.get_mut::<SpanTiming>() {
                timing.entered = Some(Instant::now());

                let namespace = &timing.namespace;
                if self.is_enabled(namespace) {
                    let (color, _) = self.get_color_and_diff(namespace);
                    let name = timing.name.clone();
                    let ts = crate::utc_timestamp();

                    let msg = format!("-> {}", name);
                    let line = if self.stderr_is_tty && self.use_colors {
                        crate::format_colored(
                            namespace,
                            crate::Level::Debug,
                            color,
                            &format_args!("{}", msg),
                            "",
                            self.show_time,
                            false,
                            &ts,
                        )
                    } else {
                        crate::format_plain(
                            namespace,
                            crate::Level::Debug,
                            &format_args!("{}", msg),
                            "",
                            false,
                            &ts,
                        )
                    };
                    let _ = io::stderr().write_all(line.as_bytes());
                }
            }
        }
    }

    fn on_exit(&self, id: &tracing_core::span::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let ext = span.extensions();
            if let Some(timing) = ext.get::<SpanTiming>() {
                let namespace = &timing.namespace;
                if self.is_enabled(namespace) {
                    let duration_str = timing
                        .entered
                        .map(|t| crate::humanize(t.elapsed().as_micros()))
                        .unwrap_or_else(|| "?".to_string());

                    let (color, _) = self.get_color_and_diff(namespace);
                    let name = &timing.name;
                    let ts = crate::utc_timestamp();

                    let msg = format!("<- {} ({})", name, duration_str);
                    let line = if self.stderr_is_tty && self.use_colors {
                        crate::format_colored(
                            namespace,
                            crate::Level::Debug,
                            color,
                            &format_args!("{}", msg),
                            "",
                            self.show_time,
                            false,
                            &ts,
                        )
                    } else {
                        crate::format_plain(
                            namespace,
                            crate::Level::Debug,
                            &format_args!("{}", msg),
                            "",
                            false,
                            &ts,
                        )
                    };
                    let _ = io::stderr().write_all(line.as_bytes());
                }
            }
        }
    }
}

/// Timing data stored in span extensions.
struct SpanTiming {
    entered: Option<Instant>,
    namespace: String,
    name: String,
}

#[cfg(test)]
mod tests {
    use super::{resolve_span_descriptor, SpanDescriptor, SpanFieldValues};

    #[test]
    fn resolve_span_descriptor_skips_rust_debug_managed_spans() {
        let result = resolve_span_descriptor(
            "app",
            "load",
            SpanFieldValues {
                rust_debug_managed: true,
                ..SpanFieldValues::default()
            },
        );

        assert_eq!(result, None);
    }

    #[test]
    fn resolve_span_descriptor_prefers_field_values() {
        let result = resolve_span_descriptor(
            "fallback",
            "default",
            SpanFieldValues {
                namespace: Some("app:init".to_string()),
                message: Some("loading config".to_string()),
                rust_debug_managed: false,
            },
        );

        assert_eq!(
            result,
            Some(SpanDescriptor {
                namespace: "app:init".to_string(),
                name: "loading config".to_string(),
            })
        );
    }
}

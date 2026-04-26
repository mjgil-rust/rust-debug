use std::time::Instant;

use crate::{enabled_for, humanize, write_log, Level};

/// A guard that logs span entry/exit with duration using rust-debug formatting.
/// Created by the [`debug_span!`] macro.
pub struct SpanGuard {
    namespace: &'static str,
    message: String,
    start: Instant,
    enabled: bool,
    #[cfg(feature = "tracing-integration")]
    _tracing_span: Option<tracing::span::EnteredSpan>,
}

impl SpanGuard {
    #[doc(hidden)]
    pub fn new(namespace: &'static str, message: &str) -> Self {
        let enabled = enabled_for(namespace, Level::Debug);
        if enabled {
            write_log(namespace, Level::Debug, format_args!("-> {}", message));
        }

        #[cfg(feature = "tracing-integration")]
        let _tracing_span = Some(internal_tracing_span(namespace, message).entered());

        Self {
            namespace,
            message: message.to_string(),
            start: Instant::now(),
            enabled,
            #[cfg(feature = "tracing-integration")]
            _tracing_span,
        }
    }
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if self.enabled {
            let elapsed = humanize(self.start.elapsed().as_micros());
            write_log(
                self.namespace,
                Level::Debug,
                format_args!("<- {} ({})", self.message, elapsed),
            );
        }
    }
}

// ─── Async instrumentation ──────────────────────────────────────────────────

/// Extension trait for attaching a namespace-aware span to a future.
///
/// ```rust,ignore
/// use rust_debug::InstrumentDebug;
///
/// async move { do_work().await }
///     .instrument_debug("vnc:save", "saving document")
///     .await;
/// ```
#[cfg(feature = "tracing-integration")]
pub trait InstrumentDebug: Sized {
    /// Attach a rust-debug namespace span to this future.
    fn instrument_debug(self, namespace: &'static str, message: &str) -> InstrumentedFuture<Self>;
}

#[cfg(feature = "tracing-integration")]
impl<F: std::future::Future> InstrumentDebug for F {
    fn instrument_debug(self, namespace: &'static str, message: &str) -> InstrumentedFuture<Self> {
        if enabled_for(namespace, Level::Debug) {
            write_log(namespace, Level::Debug, format_args!("-> {}", message));
        }

        InstrumentedFuture {
            inner: tracing::Instrument::instrument(self, internal_tracing_span(namespace, message)),
            namespace,
            message: message.to_string(),
            start: Instant::now(),
        }
    }
}

#[cfg(feature = "tracing-integration")]
fn internal_tracing_span(namespace: &'static str, message: &str) -> tracing::Span {
    tracing::info_span!(
        target: "rust_debug",
        "rust_debug_span",
        rust_debug_managed = true,
        ns = namespace,
        msg = %message
    )
}

#[cfg(feature = "tracing-integration")]
/// A future wrapped with a rust-debug namespace span.
pub struct InstrumentedFuture<F> {
    inner: tracing::instrument::Instrumented<F>,
    namespace: &'static str,
    message: String,
    start: Instant,
}

// We don't have pin-project, so use a manual implementation
#[cfg(feature = "tracing-integration")]
impl<F: std::future::Future> std::future::Future for InstrumentedFuture<F> {
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: we never move `inner` out of `self`
        let this = unsafe { self.get_unchecked_mut() };
        let inner = unsafe { std::pin::Pin::new_unchecked(&mut this.inner) };
        inner.poll(cx)
    }
}

#[cfg(feature = "tracing-integration")]
impl<F> Drop for InstrumentedFuture<F> {
    fn drop(&mut self) {
        if enabled_for(self.namespace, Level::Debug) {
            let elapsed = humanize(self.start.elapsed().as_micros());
            write_log(
                self.namespace,
                Level::Debug,
                format_args!("<- {} ({})", self.message, elapsed),
            );
        }
    }
}

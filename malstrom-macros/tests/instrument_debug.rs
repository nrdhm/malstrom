//! Verifies that `#[instrument_debug]` records spans at `DEBUG`, forwards
//! `tracing::instrument` arguments, and honours an explicit `level`.

use std::sync::{Arc, Mutex};

use malstrom_macros::instrument_debug;
use tracing::{Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry;

/// Records the level of every span that is created.
#[derive(Clone, Default)]
struct SpanLevels(Arc<Mutex<Vec<Level>>>);

impl<S: Subscriber> Layer<S> for SpanLevels {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::Id,
        _ctx: Context<'_, S>,
    ) {
        self.0.lock().unwrap().push(*attrs.metadata().level());
    }
}

/// Runs `f` with a subscriber that records span levels.
fn span_levels_of(f: impl FnOnce()) -> Vec<Level> {
    let recorder = SpanLevels::default();
    let subscriber = registry().with(recorder.clone());
    tracing::subscriber::with_default(subscriber, f);
    let levels = recorder.0.lock().unwrap().clone();
    levels
}

/// An argument that is deliberately not `Debug`, to prove `skip_all` is
/// forwarded to `tracing::instrument`.
struct NotDebug;

#[instrument_debug(skip_all)]
fn bare() {}

#[instrument_debug(skip_all)]
fn with_skipped_arg(_not_debug: NotDebug) {}

#[instrument_debug(skip_all, fields(answer = 42))]
fn with_fields() {}

#[instrument_debug(skip_all, level = "TRACE")]
fn with_explicit_level() {}

#[test]
fn defaults_to_debug() {
    let levels = span_levels_of(bare);
    assert_eq!(levels, vec![Level::DEBUG]);
}

#[test]
fn forwards_skip_all() {
    // Compiling is the assertion: without the forwarded `skip_all`,
    // `tracing::instrument` would require `NotDebug: Debug`.
    let levels = span_levels_of(|| with_skipped_arg(NotDebug));
    assert_eq!(levels, vec![Level::DEBUG]);
}

#[test]
fn forwards_fields() {
    let levels = span_levels_of(with_fields);
    assert_eq!(levels, vec![Level::DEBUG]);
}

#[test]
fn honours_explicit_level() {
    let levels = span_levels_of(with_explicit_level);
    assert_eq!(levels, vec![Level::TRACE]);
}

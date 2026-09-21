use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::sync::OnceLock;
use tracing_subscriber::{fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt};

/// Installs a `tracing` subscriber for tests, safe to call more than once.
///
/// The filter is read from `RUST_LOG` (e.g. `RUST_LOG=trace`) and defaults to `debug`
/// when the variable is unset, so `#[instrument_debug]` spans are visible by default.
/// Output goes through the test harness writer, so it is shown on failure or when
/// running with `--nocapture`.
pub fn init_logs() {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));
    // let _ = fmt().with_env_filter(filter).with_test_writer().try_init();
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_span_events(FmtSpan::CLOSE)
                .with_test_writer(),
        )
        .try_init();
}

/// Keeps the tracer provider alive for the whole test process.
static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

/// The full OTLP/HTTP traces endpoint, read from
/// `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`
/// (e.g. `http://localhost:4318/v1/traces`). When unset, only local logs are
/// installed.
fn tempo_endpoint() -> Option<String> {
    std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").ok()
}

/// Builds an OTLP HTTP exporter pointed at the configured Tempo traces
/// endpoint. The endpoint is a full OTLP/HTTP URL and is used verbatim.
///
/// `with_batch_exporter` is required here: the default OTLP HTTP client is a
/// blocking reqwest client, so spans must not be ended/exported from a Tokio
/// runtime thread. The batch processor exports on its own dedicated thread,
/// which works from the test's `rt.execute()` runtime.
fn tempo_provider(endpoint: &str) -> &'static SdkTracerProvider {
    use opentelemetry_otlp::WithExportConfig;

    TRACER_PROVIDER.get_or_init(|| {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()
            .expect("failed to build OTLP span exporter for Tempo");
        SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(
                Resource::builder()
                    .with_service_name("malstrom-operators-tests")
                    .build(),
            )
            .build()
    })
}

fn tempo_tracer(endpoint: &str) -> opentelemetry_sdk::trace::Tracer {
    tempo_provider(endpoint).tracer("malstrom-operators-tests")
}

/// Flush queued spans on the test thread (outside the tokio runtime) so
/// traces reach Tempo before the test process exits.
pub fn temp_force_flush() {
    if let Some(endpoint) = tempo_endpoint() {
        let _ = tempo_provider(&endpoint).force_flush();
    }
}

/// Installs a `tracing` subscriber for tests, safe to call more than once.
///
/// The filter is read from `RUST_LOG` (e.g. `RUST_LOG=trace`) and defaults to `debug`
/// when the variable is unset, so `#[instrument_debug]` spans are visible by default.
/// Output goes through the test harness writer, so it is shown on failure or when
/// running with `--nocapture`.
///
/// If `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` is set (see [`tempo_endpoint`]), spans
/// are exported to it via OTLP/HTTP as well.
pub fn tempo_init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));
    let subscriber = tracing_subscriber::registry().with(filter).with(
        fmt::layer()
            .with_span_events(FmtSpan::CLOSE)
            .with_test_writer(),
    );

    if let Some(endpoint) = tempo_endpoint() {
        let tracer = tempo_tracer(&endpoint);
        let _ = subscriber
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .try_init();
    } else {
        let _ = subscriber.try_init();
    }
}

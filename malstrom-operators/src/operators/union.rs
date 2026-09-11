use malstrom_core::channels::operator_io::Input;
use malstrom_core::stream::StreamBuilder;
use malstrom_core::types::{Kvt, MaybeData, MaybeKey, MaybeTime, Sealed};

/// Trait with union() method.
pub trait Union<Msg: Kvt>: Sealed {
    /// Merge self with the given streams into one.
    fn union(
        self,
        name: impl Into<String>,
        inputs: impl IntoIterator<Item = StreamBuilder<Msg>>,
    ) -> StreamBuilder<Msg>;
}

impl<Msg> Union<Msg> for StreamBuilder<Msg>
where
    Msg: Kvt,
    Msg::Key: MaybeKey,
    Msg::Value: MaybeData,
    Msg::Timestamp: MaybeTime,
{
    fn union(
        mut self,
        name: impl Into<String>,
        inputs: impl IntoIterator<Item = StreamBuilder<Msg>>,
    ) -> StreamBuilder<Msg> {
        let mut united_input = Input::new_unlinked();
        let name: String = name.into();

        self.forward_tail_to(format!("{}-0", name), &mut united_input);

        for (i, mut stream) in inputs.into_iter().enumerate() {
            stream.forward_tail_to(format!("{}-{}", name, i + 1), &mut united_input);
        }
        self.with_new_tail(united_input)
    }
}

#[cfg(test)]
mod tests {
    use crate::operators::Source as _;
    use crate::operators::*;
    use crate::sinks::StatelessSink;
    use crate::sinks::VecSink;
    use crate::sources::Source;
    use indexmap::IndexSet;
    use malstrom_testkit::get_test_rt;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use opentelemetry_sdk::Resource;
    use std::sync::OnceLock;
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    /// Tempo OTLP endpoint; overridable with `OTEL_EXPORTER_OTLP_ENDPOINT` or
    /// `TEMPO_OTLP_ENDPOINT`. When unset, only local logs are installed.
    const TEMPO_OTLP_ENDPOINT: &str = "http://localhost:4318";

    /// Keeps the tracer provider alive for the whole test process.
    static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

    fn tempo_endpoint() -> Option<String> {
        std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .or_else(|| std::env::var("TEMPO_OTLP_ENDPOINT").ok())
            .or_else(|| (!TEMPO_OTLP_ENDPOINT.is_empty()).then(|| TEMPO_OTLP_ENDPOINT.to_string()))
    }

    /// Builds an OTLP HTTP exporter pointed at the configured Tempo endpoint.
    ///
    /// `with_simple_exporter` exports synchronously when a span closes, which avoids
    /// requiring a Tokio runtime and flushes spans before the test process exits.
    fn tempo_tracer(endpoint: &str) -> opentelemetry_sdk::trace::Tracer {
        use opentelemetry_otlp::WithExportConfig;

        let provider = TRACER_PROVIDER.get_or_init(|| {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_endpoint(endpoint)
                .build()
                .expect("failed to build OTLP span exporter for Tempo");
            SdkTracerProvider::builder()
                .with_simple_exporter(exporter)
                .with_resource(
                    Resource::builder()
                        .with_service_name("malstrom-operators-tests")
                        .build(),
                )
                .build()
        });
        provider.tracer("malstrom-operators-tests")
    }

    /// Installs a `tracing` subscriber for tests, safe to call more than once.
    ///
    /// The filter is read from `RUST_LOG` (e.g. `RUST_LOG=debug`) and defaults to `info`
    /// when the variable is unset. Output goes through the test harness writer, so it is
    /// shown on failure or when running with `--nocapture`.
    ///
    /// If a Tempo endpoint is configured (see [`tempo_endpoint`]), spans are exported to
    /// it via OTLP/HTTP as well.
    pub fn init_logs() {
        use tracing_subscriber::{EnvFilter, fmt};

        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(
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

    #[test]
    fn union_unites() {
        init_logs();
        let collector = VecSink::new();
        let rt = get_test_rt(|provider| {
            let a = provider
                .new_stream()
                .source("source-a", Source::from_iterator(0..10));
            let b = provider
                .new_stream()
                .source("source-b", Source::from_iterator(10..20));
            b.union("fan-in", vec![a])
                .sink("sink", StatelessSink::new(collector.clone()));
        });
        rt.execute().unwrap();

        let collected: IndexSet<usize> = collector.into_iter().map(|x| x.value).collect();
        // The order of values is not specified; they appear as available.
        // TODO: debug high latency before ending.
        let expected: IndexSet<usize> = (10..20).chain(0..10).collect();
        assert_eq!(expected, collected)
    }
}

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
    use tracing_subscriber::fmt::format::FmtSpan;

    /// Installs a `tracing` subscriber for tests, safe to call more than once.
    ///
    /// The filter is read from `RUST_LOG` (e.g. `RUST_LOG=debug`) and defaults to `info`
    /// when the variable is unset. Output goes through the test harness writer, so it is
    /// shown on failure or when running with `--nocapture`.
    pub fn init_logs() {
        use tracing_subscriber::{EnvFilter, fmt};

        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = fmt()
            .with_env_filter(filter)
            .with_span_events(FmtSpan::CLOSE)
            .with_test_writer()
            .try_init();
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

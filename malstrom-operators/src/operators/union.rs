use malstrom_core::channels::operator_io::Input;
use malstrom_core::stream::{Forward, OperatorBuilder, SafeLogic, StreamBuilder};
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
        let name: String = name.into();
        // all streams sink here
        let mut united_input = Input::new_unlinked();
        // first edge to the sink
        let mut edge = OperatorBuilder::new(format!("{}-0", name).into())
            // just a dummy operator to connect tail (input) with the united input
            .with_direct_logic(Forward::new().into_logic())
            .build();
        // redirect the dataflow into the edge.
        self.swap_tail(&mut edge.input);
        // connect the edge output to the united_input.
        edge.link_to_input(&mut united_input);
        // register the edge as a runtime task
        self.add_operator(edge);

        // each other stream goes thru the same process
        for (i, mut stream) in inputs.into_iter().enumerate() {
            let forwarder = Forward::<Msg>::new().into_logic();
            let mut edge = OperatorBuilder::new(format!("{}-{}", name, i + 1).into())
                .with_direct_logic(forwarder)
                .build();
            // redirect to the edge
            stream.swap_tail(&mut edge.input);
            // connect the edge to the united_input
            edge.link_to_input(&mut united_input);
            // don't forget to register in the runtime
            self.add_operator(edge);
        }

        // the united stream builder
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
    use malstrom_testkit::test_support::init_logs;
    use malstrom_testkit::test_support::temp_force_flush;
    use malstrom_testkit::test_support::tempo_init_tracing;
    #[test]
    fn union_unites() {
        init_logs();
        tempo_init_tracing();
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

        temp_force_flush();

        let collected: IndexSet<usize> = collector.into_iter().map(|x| x.value).collect();
        // The order of values is not specified; they appear as available.
        // Not a TODO: the ~5s wall time before the test ends is the coordinator's completion
        // poll interval, not union latency — the trace shows ~1ms busy and ~5s idle waiting on
        // the poll (see docs/reviews/2026-09-21-pre-commit-assessment.md).
        let expected: IndexSet<usize> = (10..20).chain(0..10).collect();
        assert_eq!(expected, collected)
    }
}

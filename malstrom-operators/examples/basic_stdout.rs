//! A basic example which runs a no-op dataflow
use malstrom_operators::operators::Source as _;
use malstrom_operators::operators::*;
use malstrom::runtime::SingleThreadRuntime;
use malstrom_operators::sinks::{StatelessSink, StdOutSink};
use malstrom::snapshot::NoPersistence;
use malstrom_operators::sources::Source;
use malstrom::worker::StreamProvider;

fn main() {
    SingleThreadRuntime::builder()
        .persistence(NoPersistence)
        .build(build_dataflow)
        .execute()
        .unwrap()
}

fn build_dataflow(provider: &mut dyn StreamProvider) -> () {
    let stream = provider.new_stream().source(
        // this is an operator
        "iter-source",
        Source::from_iterator(0..=10),
    );
    stream.sink("iter-sink", StatelessSink::new(StdOutSink));
}

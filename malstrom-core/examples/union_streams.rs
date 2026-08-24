//! Combining multiple streams
use malstrom::operators::Source as _;
use malstrom::operators::*;
use malstrom::runtime::SingleThreadRuntime;
use malstrom::sinks::{StatelessSink, StdOutSink};
use malstrom::snapshot::NoPersistence;
use malstrom::sources::Source;
use malstrom::worker::StreamProvider;

fn main() {
    SingleThreadRuntime::builder()
        .persistence(NoPersistence)
        .build(build_dataflow)
        .execute()
        .unwrap()
}

fn build_dataflow(provider: &mut dyn StreamProvider) -> () {
    let numbers = provider
        .new_stream()
        .source("iter-source", Source::from_iterator(0..=10));
    let more_numbers = provider
        .new_stream()
        .source("other-iter-source", Source::from_iterator(0..=10));

    numbers
        .union("union all", [more_numbers])
        .sink("std-out-sink", StatelessSink::new(StdOutSink));
}

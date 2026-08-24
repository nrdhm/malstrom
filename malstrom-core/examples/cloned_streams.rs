//! Combining multiple streams
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
    let [numbers, more_numbers] = provider
        .new_stream()
        .source("iter-source", Source::from_iterator(0..=100))
        .const_cloned("clone-values");

    numbers.sink("numbers-sink", StatelessSink::new(StdOutSink));
    more_numbers.sink("more-numbers-sink", StatelessSink::new(StdOutSink));
}

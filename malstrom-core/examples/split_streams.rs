//! Combining multiple streams
use malstrom::operators::*;
use malstrom::operators::Source as _;
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
    let [even_nums, all_nums] = provider
        .new_stream()
        .source(
            "iter-source",
            Source::from_iterator(0..=100),
        )
        .const_split("split-even-odd", |msg, outputs| {
            if msg.value & 1 == 0 {
                // is even
                *outputs = [true, true]
            } else {
                *outputs = [false, true]
            }
        });

    even_nums.sink("even-sink", StatelessSink::new(StdOutSink));
    all_nums.sink("all-sink", StatelessSink::new(StdOutSink));
}

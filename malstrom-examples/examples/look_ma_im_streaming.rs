//! Stream processing can be easy!
use malstrom::operators::Source as _;
use malstrom::operators::*;
use malstrom::runtime::MultiThreadRuntime;
use malstrom::sinks::{StatelessSink, StdOutSink};
use malstrom::snapshot::NoPersistence;
use malstrom::sources::Source;
use malstrom::worker::StreamProvider;

fn main() {
    MultiThreadRuntime::builder()
        .persistence(NoPersistence)
        .parrallelism(1)
        .build(build_dataflow)
        .execute()
        .unwrap();
}

fn build_dataflow(provider: &mut dyn StreamProvider) {
    provider
        .new_stream()
        .source(
            "words",
            Source::from_iterator([
                "Look".to_string(),
                "ma'".to_string(),
                "I'm".to_string(),
                "streaming".to_string(),
            ]),
        )
        .map("upper", async |x| x.to_uppercase())
        .sink("stdout", StatelessSink::new(StdOutSink));
}

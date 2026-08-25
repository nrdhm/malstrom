//! A multithreaded, keyed stream
use malstrom::keyed::rendezvous_select;
use malstrom::operators::Source as _;
use malstrom::operators::*;
use malstrom::runtime::MultiThreadRuntime;
use malstrom::snapshot::NoPersistence;
use malstrom::sources::Source;
use malstrom::worker::StreamProvider;

fn main() {
    MultiThreadRuntime::builder()
        .parrallelism(1)
        .persistence(NoPersistence)
        .build(build_dataflow)
        .execute()
        .unwrap()
}

fn build_dataflow(provider: &mut dyn StreamProvider) -> () {
    provider
        .new_stream()
        .source("iter-source", Source::from_iterator(0..=100))
        .key_distribute("key-odd-even", |x| (x.value & 1) == 0, rendezvous_select)
        .inspect("print", async |x, ctx| {
            println!("{x:?} @ Worker {}", ctx.worker_id)
        });
}

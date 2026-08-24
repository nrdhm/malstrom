//! A multithreaded program
use malstrom_operators::keyed::rendezvous_select;
use malstrom_operators::operators::Source as _;
use malstrom_operators::operators::*;
use malstrom::runtime::{MultiThreadRuntime, SingleThreadRuntime};
use malstrom::snapshot::NoPersistence;
use malstrom_operators::sources::Source;
use malstrom::worker::StreamProvider;

fn main() {
    console_subscriber::init();
    // SingleThreadRuntime::builder()
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
        .source("iter-source", Source::from_iterator(0..=10))
        // .key_distribute("key-by-value", |x| x.value, rendezvous_select)
        // .map("double", async |x| x * 2)
        .inspect("print", async |x, ctx| {
            println!("{x:?} @ Worker {}", ctx.worker_id)
        });
}

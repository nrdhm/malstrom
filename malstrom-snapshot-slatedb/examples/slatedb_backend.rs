//! Using SlateDB as a persistence backend
use malstrom_combinators::combinators::Source as _;
use malstrom_combinators::combinators::*;
use malstrom_combinators::keyed::rendezvous_select;
use malstrom_combinators::sinks::{StatelessSink, StdOutSink};
use malstrom_snapshot_slatedb::object_store::{local::LocalFileSystem, path::Path};
use std::sync::Arc;
use std::time::Duration;
use {
    malstrom_combinators::sources::Source, malstrom_core::runtime::SingleThreadRuntime,
    malstrom_core::worker::StreamProvider, malstrom_snapshot_slatedb::SlateDbBackend,
};

fn main() {
    let filesystem = LocalFileSystem::new();
    let persistence = SlateDbBackend::new(Arc::new(filesystem), Path::from("/tmp")).unwrap();

    SingleThreadRuntime::builder()
        .persistence(persistence)
        .snapshots(Duration::from_secs(10))
        .build(build_dataflow)
        .execute()
        .unwrap()
}

fn build_dataflow(provider: &mut dyn StreamProvider) {
    provider
        .new_stream()
        .source("iter-source", Source::from_iterator(0..=100))
        .key_distribute("key-by-value", |x| x.value & 1 == 1, rendezvous_select)
        .stateful_map("sum", async |_key, value, state: i32| {
            let state = state + value;
            (state, Some(state))
        })
        .sink("stdout", StatelessSink::new(StdOutSink));
}

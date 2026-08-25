//! Public-API contract: a full `malstrom::` pipeline (`sources` → `operators` →
//! `sinks`) runs on both runtimes with deterministic output. This pins the facade
//! re-exports and the end-user dataflow API from the outside — a missing facade
//! re-export or a broken operator wiring fails here at compile or run time.

use malstrom::operators::Source as _;
use malstrom::operators::*;
use malstrom::runtime::{MultiThreadRuntime, SingleThreadRuntime};
use malstrom::sinks::{StatelessSink, VecSink};
use malstrom::snapshot::NoPersistence;
use malstrom::sources::Source;
use malstrom::types::{DataMessage, NoKey, OnceTime};
use malstrom::worker::StreamProvider;

type Record = DataMessage<(NoKey, i32, OnceTime)>;

fn build_dataflow(provider: &mut dyn StreamProvider, sink: VecSink<Record>) {
    provider
        .new_stream()
        .source("numbers", Source::from_iterator(0..=10))
        .map("double", async |x| x * 2)
        .sink("collect", StatelessSink::new(sink));
}

#[test]
fn pipeline_runs_on_single_thread_runtime() {
    let sink = VecSink::new();
    let sink_test = sink.clone();
    SingleThreadRuntime::builder()
        .persistence(NoPersistence)
        .build(move |p: &mut dyn StreamProvider| build_dataflow(p, sink))
        .execute()
        .unwrap();

    let values: Vec<i32> = sink_test.drain_vec(..).into_iter().map(|d| d.value).collect();
    assert_eq!(values, (0..=10).map(|x| x * 2).collect::<Vec<_>>());
}

#[test]
fn pipeline_runs_on_multi_thread_runtime() {
    let sink = VecSink::new();
    let sink_test = sink.clone();
    MultiThreadRuntime::builder()
        .parrallelism(1)
        .persistence(NoPersistence)
        .build(move |p: &mut dyn StreamProvider| build_dataflow(p, sink.clone()))
        .execute()
        .unwrap();

    let values: Vec<i32> = sink_test.drain_vec(..).into_iter().map(|d| d.value).collect();
    assert_eq!(values, (0..=10).map(|x| x * 2).collect::<Vec<_>>());
}

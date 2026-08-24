//! Using SlateDB as a persistence backend
use malstrom_operators::keyed::rendezvous_select;
use malstrom_operators::operators::*;
use malstrom_operators::sinks::{StatelessSink, StdOutSink};
use malstrom::snapshot::slatedb::object_store::{local::LocalFileSystem, path::Path};
use malstrom_operators::sources::{Source, SourceImpl, SourcePartition};
use malstrom::{runtime::SingleThreadRuntime, snapshot::SlateDbBackend, worker::StreamProvider};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn main() {
    let filesystem = LocalFileSystem::new();
    let persistence = SlateDbBackend::new(Arc::new(filesystem), Path::from("/tmp")).unwrap();

    loop {
        let job = SingleThreadRuntime::builder()
            .persistence(persistence.clone())
            .snapshots(Duration::from_secs(1))
            .build(build_dataflow);
        let thread = std::thread::spawn(move || job.execute().unwrap());
        match thread.join() {
            Ok(_) => return,
            Err(_) => {
                println!("Restarting worker");
                continue;
            }
        }
    }
}

fn build_dataflow(provider: &mut dyn StreamProvider) {
    let start_time = Instant::now();
    let fail_interval = Duration::from_secs(10);
    provider
        .new_stream()
        .source("iter-source", Source::from_impl(StatefulNumberSource(0)))
        .key_distribute("key-by-value", |x| x.value & 1 == 1, rendezvous_select)
        .stateful_map("sum", |_key, value, state: i32| {
            let state = state + value;
            (state, Some(state))
        })
        .inspect("expensive-operation", |_msg, _ctx| {
            // we need this to not overflow the sum before "crashing"
            sleep(Duration::from_millis(100))
        })
        .inspect("fail-random", move |_msg, _ctx| {
            if Instant::now().duration_since(start_time) > fail_interval {
                panic!("Oh no!")
            }
        })
        .sink("stdout", StatelessSink::new(StdOutSink));
}

struct StatefulNumberSource(i32);

impl SourceImpl for StatefulNumberSource {
    type PartitionKey = ();
    type Value = i32;
    type Timestamp = i32;
    type PartitionState = i32;
    type Partition = Self;

    async fn discover(&mut self) -> Vec<Self::PartitionKey> {
        vec![()]
    }

    async fn open(
        &mut self,
        _part: &Self::PartitionKey,
        part_state: Option<Self::PartitionState>,
    ) -> Self::Partition {
        println!("Build with {part_state:?}");
        Self(part_state.unwrap_or_default())
    }
}

impl SourcePartition for StatefulNumberSource {
    type PartitionKey = ();
    type Value = i32;
    type Timestamp = i32;
    type State = i32;

    async fn poll(&mut self) -> Option<(Self::Value, Self::Timestamp)> {
        let out = Some((self.0, self.0));
        self.0 += 1;
        out
    }

    async fn snapshot(&self) -> Self::State {
        println!("SNAPSHOTTING SOURCE");
        self.0
    }

    async fn collect(self) -> Self::State {
        self.0
    }
}

//! Compile-time contract: every historical top-level path resolves through the
//! `malstrom::` facade. A missing re-export fails this file at compile time — this is
//! the cheapest "public API predictability" anchor for users, covering the facade
//! (kernel + operators + distributed under one namespace).

#![allow(unused_imports)]

// kernel modules re-exported by the facade
use malstrom::channels::alignment::AlignmentGroup;
use malstrom::channels::operator_io::{Input, Output, full_broadcast, link};
use malstrom::channels::recv_trait::Receiver;
use malstrom::channels::spsc;
use malstrom::coordinator::{
    ApiRequestError, Coordinator, CoordinatorApi, CoordinatorExecutionError,
};
use malstrom::runtime::communication::{
    OperatorOperatorComm, ReqResReceiver, ReqResResponder, ReqResSender, StreamReceiver,
    StreamSender, WorkerCoordinatorComm,
};
use malstrom::runtime::{MultiThreadRuntime, RuntimeFlavor, SingleThreadRuntime};
use malstrom::snapshot::{
    NoPersistence, PersistenceBackend, PersistenceClient, SnapshotBarrier, SnapshotVersion,
    deserialize_state, serialize_state,
};
use malstrom::stream::{
    BuildContext, DirectLogic, InitialStreamBuilder, Logic, LogicBuilder, Malstrom, Operator,
    OperatorContext, SafeLogic, SafeLogicWrapper, StreamBuilder,
};
use malstrom::types::distributable::Distributable;
use malstrom::types::{
    Barrier, Data, DataMessage, Key, Kvt, MaybeData, MaybeKey, MaybeTime, Message, NoData, NoKey,
    NoTime, OnceTime, OperatorId, OperatorPartitioner, ReconfigComplete, RescaleMessage,
    SuspendMarker, Timestamp,
};
use malstrom::worker::{InnerRuntimeBuilder, StreamProvider, Worker, WorkerBuilder};

// operator-layer modules re-exported by the facade
use malstrom::keyed::distributed::rendezvous_select;
use malstrom::keyed::{KeyDistribute, KeyLocal, WorkerBroadcast, WorkerPartitioner};
use malstrom::operators::{
    Cloned, Filter, FilterMap, Flatten, Inspect, Map, Sink, Source as _, Split, StatefulMap,
    TtlMap, Union,
};
use malstrom::sinks::{StatefulSink, StatelessSink, StdOutSink, VecSink};
use malstrom::sources::{FromIteratorSource, Source, SourceImpl, SourcePartition};

/// Touch the imports so the file doubles as a smoke check, not just a compile check.
#[test]
fn historical_paths_resolve_and_are_usable() {
    // snapshot helpers exist and round-trip
    let bytes = serialize_state(&42u32);
    assert_eq!(deserialize_state::<u32>(bytes), 42);

    // the types namespace exposes the tuple Kvt impl
    fn assert_kvt<M: Kvt>() {}
    assert_kvt::<(i32, i32, i32)>();

    // runtime types are nameable
    fn assert_flavor<F: RuntimeFlavor>() {}
    let _ = std::any::type_name::<SingleThreadRuntime<malstrom::snapshot::NoPersistence, ()>>();
    let _ = std::any::type_name::<MultiThreadRuntime<malstrom::snapshot::NoPersistence, ()>>();
    let _ = assert_flavor::<malstrom::runtime::SingleThreadRuntimeFlavor>;
}

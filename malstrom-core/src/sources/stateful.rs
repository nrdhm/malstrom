//! The source engine: one `SourceImpl` abstraction plus the operator graph that
//! discovers, distributes and reads partitions. "Stateless" sources are simply
//! `SourceImpl` implementations with `PartitionState = ()` — see the
//! `Source::from_*` constructors in [`crate::sources::fn_source`].

use std::{cell::RefCell, rc::Rc};

use futures::{StreamExt, stream::FuturesUnordered};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use crate::{
    channels::operator_io::{Input, Output},
    keyed::{
        Distribute as _,
        distributed::{Acquire, Collect, Interrogate},
        rendezvous_select,
    },
    operators::{CommUtility, StreamSource},
    stream::{
        BuildContext, InitialStreamBuilder, Logic, LogicBuilder, Malstrom as _, Operator,
        OperatorContext, SafeLogic, SafeLogicWrapper, StreamBuilder,
    },
    types::{
        Barrier, Data, DataMessage, Key, Kvt, Message, NoData, Timestamp, WorkerId,
        distributable::Distributable,
    },
};

/// A partitioned, possibly stateful source.
///
/// Stateless sources set `PartitionState = ()` (or use the `Source::from_*`
/// constructors) and implement `snapshot`/`collect` as no-ops.
pub trait SourceImpl: 'static {
    /// Identifies a partition (one shard / split / file / topic-partition / …).
    type PartitionKey: Distributable + Key;
    /// Values this source emits.
    type Value: Distributable + Data;
    /// Timestamps this source emits.
    type Timestamp: Distributable + Timestamp;
    /// Per-partition state persisted across restarts and moved on rescale.
    /// `()` = stateless.
    type PartitionState: Distributable;
    /// The reader produced by [SourceImpl::open].
    type Partition: SourcePartition<
        PartitionKey = Self::PartitionKey,
        Value = Self::Value,
        Timestamp = Self::Timestamp,
        State = Self::PartitionState,
    >;

    /// Discover the partitions this source exposes.
    ///
    /// Called by the framework at build time (worker 0) and again on rescale.
    /// May perform I/O (list a bucket, query a broker, …).
    async fn discover(&mut self) -> Vec<Self::PartitionKey>;

    /// Open a reader for `key`, resuming from `state` if one was persisted.
    ///
    /// The framework calls this for each discovered (or restored) partition.
    async fn open(
        &mut self,
        key: &Self::PartitionKey,
        state: Option<Self::PartitionState>,
    ) -> Self::Partition;
}

/// One partition reader. The framework owns its lifecycle.
pub trait SourcePartition {
    /// Identifies this partition.
    type PartitionKey: Distributable + Key;
    /// Values this partition emits.
    type Value: Distributable + Data;
    /// Timestamps this partition emits.
    type Timestamp: Distributable + Timestamp;
    /// Resume state captured by [SourcePartition::snapshot].
    type State: Distributable;

    /// Poll this partition; return `None` once no further records will be
    /// produced by it. MUST be cancel-safe.
    async fn poll(&mut self) -> Option<(Self::Value, Self::Timestamp)>;

    /// Capture the state to resume from later.
    async fn snapshot(&self) -> Self::State;

    /// Shut down and return the final state (moves to another worker / job end).
    async fn collect(self) -> Self::State;
}

/// A source providing records for processing. Wrap a [SourceImpl] with one of the
/// `Source::from_*` constructors (from [crate::sources::fn_source]) or
/// [Source::from_impl].
pub struct Source<SrcImpl>(SrcImpl);

/// The `from_*` constructors. `Source<()>` is only a carrier so the methods can be
/// called as `Source::from_iterator(…)`; the actual source is `Source<SrcImpl>`.
impl Source<()> {
    /// Create a source from a [SourceImpl] implementation.
    pub fn from_impl<SrcImpl: SourceImpl>(source: SrcImpl) -> Source<SrcImpl> {
        Source(source)
    }

    /// An untimed source reading from an iterator. Every record is timestamped
    /// [`OnceTime(false)`](crate::types::OnceTime); the stream finishes with
    /// `OnceTime(true)`. For index timestamps see
    /// [Source::from_enumerated_iterator].
    pub fn from_iterator<V>(
        iter: impl IntoIterator<Item = V> + 'static,
    ) -> Source<crate::sources::fn_source::FromIteratorSource<V>>
    where
        V: Distributable + Data,
    {
        Source(crate::sources::fn_source::FromIteratorSource::new(iter))
    }

    /// A source reading from an iterator, timestamping each record with its index.
    pub fn from_enumerated_iterator<V>(
        iter: impl IntoIterator<Item = V> + 'static,
    ) -> Source<crate::sources::fn_source::FromEnumeratedIteratorSource<V>>
    where
        V: Distributable + Data,
    {
        Source(crate::sources::fn_source::FromEnumeratedIteratorSource::new(iter))
    }

    /// A source built from a poll closure returning `Option<(Value, Timestamp)>`.
    pub fn from_poll_fn<V, T, Fut>(
        f: impl FnMut() -> Fut + 'static,
    ) -> Source<crate::sources::fn_source::PollSource<V, T, impl FnMut() -> Fut>>
    where
        V: Distributable + Data,
        T: Distributable + Timestamp,
        Fut: std::future::Future<Output = Option<(V, T)>>,
    {
        Source(crate::sources::fn_source::PollSource::new(f))
    }

    /// A source reading from a `Stream` of `(value, timestamp)` pairs.
    pub fn from_stream<V, T, S>(
        stream: S,
    ) -> Source<crate::sources::fn_source::FromStreamSource<V, T, S>>
    where
        V: Distributable + Data,
        T: Distributable + Timestamp,
        S: futures::Stream<Item = (V, T)> + 'static,
    {
        Source(crate::sources::fn_source::FromStreamSource::new(stream))
    }
}

impl<SrcImpl> StreamSource<(SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp)>
    for Source<SrcImpl>
where
    SrcImpl: SourceImpl,
{
    fn into_stream(
        self,
        name: &str,
        builder: InitialStreamBuilder,
    ) -> StreamBuilder<(SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp)> {
        let src_impl = Rc::new(RefCell::new(self.0));
        // a per-source comm channel id so concurrent sources on one worker do not collide
        let comm_channel = seahash::hash(name.as_bytes());
        let coordinator = SourceCoordinatorBuilder::new(Rc::clone(&src_impl), comm_channel);
        let reader = SourcePartitionOpBuilder::new(src_impl, comm_channel);
        builder
            // worker 0 discovers the partitions once and hands them to the distribute step
            .then(Operator::built_by(
                format!("{name}-list-partitions"),
                coordinator,
            ))
            .distribute(format!("{name}-distribute-partitions"), rendezvous_select)
            .then(Operator::built_by(format!("{name}-partition"), reader))
    }
}

/// The discovery coordinator. On worker 0 it discovers the partitions once, hands
/// them to the distribute step, and emits the final `Epoch(MAX)` once every
/// partition has reported finished (via per-source comm channels). Keeping the
/// global part set here is what makes the MAX epoch correct across workers —
/// a reader emitting MAX on its own exhaustion would race with partitions still
/// in flight through the distribute step.
struct SourceCoordinatorBuilder<SrcImpl: SourceImpl> {
    source_impl: Rc<RefCell<SrcImpl>>,
    comm_channel: u64,
}

impl<SrcImpl> SourceCoordinatorBuilder<SrcImpl>
where
    SrcImpl: SourceImpl,
{
    fn new(source_impl: Rc<RefCell<SrcImpl>>, comm_channel: u64) -> Self {
        Self {
            source_impl,
            comm_channel,
        }
    }
}

impl<SrcImpl>
    LogicBuilder<(), (SrcImpl::PartitionKey, NoData, SrcImpl::Timestamp)>
    for SourceCoordinatorBuilder<SrcImpl>
where
    SrcImpl: SourceImpl,
{
    type Logic = SourceCoordinator<SrcImpl>;

    async fn build(self, ctx: &mut BuildContext) -> Self::Logic {
        let comm = CommUtility::new(ctx, self.comm_channel).await;
        // only worker 0 discovers; the distribute step then hands the parts out
        let mut parts = IndexSet::new();
        if ctx.worker_id == 0 {
            parts = self
                .source_impl
                .borrow_mut()
                .discover()
                .await
                .into_iter()
                .collect();
        }
        SourceCoordinator {
            parts,
            listed_parts: ctx.worker_id == 0,
            sent: false,
            source_impl: self.source_impl,
            comm,
        }
    }
}

struct SourceCoordinator<SrcImpl: SourceImpl> {
    parts: IndexSet<SrcImpl::PartitionKey>,
    /// whether this coordinator discovered the parts (only worker 0 does)
    listed_parts: bool,
    /// whether the parts have been handed to the distribute step yet
    sent: bool,
    source_impl: Rc<RefCell<SrcImpl>>,
    /// communication to the reader ops on all workers
    comm: CommUtility<PartitionFinished<SrcImpl::PartitionKey>>,
}

impl<SrcImpl> Logic<(), (SrcImpl::PartitionKey, NoData, SrcImpl::Timestamp)>
    for SourceCoordinator<SrcImpl>
where
    SrcImpl: SourceImpl,
{
    async fn apply(
        &mut self,
        input: &mut Input<()>,
        output: &mut Output<(SrcImpl::PartitionKey, NoData, SrcImpl::Timestamp)>,
        ctx: &mut OperatorContext,
    ) {
        // hand the discovered partitions to the distribute step exactly once
        if !self.sent {
            self.sent = true;
            for part in &self.parts {
                debug_assert!(ctx.worker_id == 0);
                let msg = DataMessage::new(part.clone(), NoData, SrcImpl::Timestamp::MIN);
                output.send(Message::Data(msg)).await;
            }
        }

        tokio::select! {
            msg = input.recv() => {
                match msg {
                    Message::Data(_) => (),
                    Message::Epoch(_) => (),
                    Message::AbsBarrier(x) => output.send(Message::AbsBarrier(x)).await,
                    Message::Rescale(x) => output.send(Message::Rescale(x)).await,
                    Message::ReconfigComplete(x) => output.send(Message::ReconfigComplete(x)).await,
                    Message::Interrogate(x) => (),
                    Message::Collect(x) => (),
                    Message::Acquire(x) => (),
                }
            }
            part_finished = self.comm.recv() => {
                let part = part_finished.0;
                self.parts.swap_remove(&part);
                // only emit the MAX epoch once every globally-known partition has finished;
                // this avoids emitting it too early when a rescale later assigns new partitions
                if self.listed_parts && self.parts.is_empty() {
                    output.send(Message::Epoch(SrcImpl::Timestamp::MAX)).await;
                }
            }
        }
    }
}

/// Marker a reader op sends to the discovery coordinator once a partition is exhausted.
#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Clone)]
struct PartitionFinished<PartitionKey>(PartitionKey);

struct SourcePartitionOp<SrcImpl: SourceImpl> {
    partitions: IndexMap<SrcImpl::PartitionKey, SrcImpl::Partition>,
    part_builder: Rc<RefCell<SrcImpl>>,
    /// communication to the discovery coordinator (worker 0)
    com_utility: CommUtility<PartitionFinished<SrcImpl::PartitionKey>>,
}

struct SourcePartitionOpBuilder<SrcImpl: SourceImpl> {
    src_impl: Rc<RefCell<SrcImpl>>,
    comm_channel: u64,
}

impl<SrcImpl> SourcePartitionOpBuilder<SrcImpl>
where
    SrcImpl: SourceImpl,
{
    fn new(src_impl: Rc<RefCell<SrcImpl>>, comm_channel: u64) -> Self {
        Self {
            src_impl,
            comm_channel,
        }
    }
}

impl<SrcImpl>
    LogicBuilder<
        (SrcImpl::PartitionKey, NoData, SrcImpl::Timestamp),
        (SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp),
    > for SourcePartitionOpBuilder<SrcImpl>
where
    SrcImpl: SourceImpl,
{
    type Logic = SafeLogicWrapper<SourcePartitionOp<SrcImpl>>;

    async fn build(self, ctx: &mut BuildContext) -> Self::Logic {
        SourcePartitionOp::new(ctx, self.src_impl, self.comm_channel)
            .await
            .into_logic()
    }
}

impl<SrcImpl> SourcePartitionOp<SrcImpl>
where
    SrcImpl: SourceImpl,
{
    async fn new(
        ctx: &mut BuildContext,
        part_builder: Rc<RefCell<SrcImpl>>,
        comm_channel: u64,
    ) -> Self {
        let partitions = IndexMap::default();
        let com_utility = CommUtility::new(ctx, comm_channel).await;
        let mut this = SourcePartitionOp {
            partitions,
            part_builder,
            com_utility,
        };

        if let Some(state) = ctx
            .load_state::<IndexMap<SrcImpl::PartitionKey, SrcImpl::PartitionState>>()
            .await
        {
            for (k, v) in state.into_iter() {
                this.add_partition(k, Some(v)).await;
            }
        }
        this
    }

    async fn add_partition(
        &mut self,
        part: SrcImpl::PartitionKey,
        part_state: Option<SrcImpl::PartitionState>,
    ) {
        if !self.partitions.contains_key(&part) {
            let partition = self
                .part_builder
                .borrow_mut()
                .open(&part, part_state)
                .await;
            self.partitions.insert(part, partition);
        }
    }
}

impl<SrcImpl>
    SafeLogic<
        (SrcImpl::PartitionKey, NoData, SrcImpl::Timestamp),
        (SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp),
    > for SourcePartitionOp<SrcImpl>
where
    SrcImpl: SourceImpl,
{
    async fn on_schedule(
        &mut self,
        output: &mut Output<(SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp)>,
        ctx: &mut OperatorContext,
    ) -> bool {
        let mut polls: FuturesUnordered<_> = self
            .partitions
            .iter_mut()
            .map(|(k, v)| async move { (k, v.poll().await) })
            .collect();
        let next_data = polls.next().await;
        drop(polls); // drop so we can modify self.partitions
        match next_data {
            // fetched data
            Some((part, Some((data, timestamp)))) => {
                let msg = DataMessage::new(part.clone(), data, timestamp);
                output.send(Message::Data(msg)).await;
                true
            }
            // partition finished
            Some((part, None)) => {
                // need to clone because part is borrowed from self.partitions which
                // we can not mutate while it is borrowed
                let part = part.clone();
                self.partitions.swap_remove(&part);
                self.com_utility
                    .send(0, PartitionFinished(part.clone()))
                    .await;
                true
            }
            // no partitions
            None => false,
        }
    }

    async fn on_data(
        &mut self,
        data_message: DataMessage<(SrcImpl::PartitionKey, NoData, SrcImpl::Timestamp)>,
        output: &mut Output<(SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp)>,
        ctx: &mut OperatorContext,
    ) {
        let partition_key = data_message.key;
        self.add_partition(partition_key, None).await;
    }

    async fn on_acquire(
        &mut self,
        acquire: &mut Acquire<SrcImpl::PartitionKey>,
        output: &mut Output<(SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp)>,
        ctx: &mut OperatorContext,
    ) {
        if let Some((part, part_state)) = acquire.take_state(&ctx.operator_id) {
            self.add_partition(part, part_state).await;
        }
    }

    async fn on_barrier(
        &mut self,
        barrier: &mut Barrier,
        output: &mut Output<(SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp)>,
        ctx: &mut OperatorContext,
    ) {
        let mut snapshot: IndexMap<SrcImpl::PartitionKey, SrcImpl::PartitionState> =
            IndexMap::with_capacity(self.partitions.len());
        for (k, v) in self.partitions.iter() {
            let state = v.snapshot().await;
            snapshot.insert(k.clone(), state);
        }
        barrier.persist(&snapshot, &ctx.operator_id);
    }

    async fn on_collect(
        &mut self,
        collect: &mut Collect<SrcImpl::PartitionKey>,
        output: &mut Output<(SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp)>,
        ctx: &mut OperatorContext,
    ) {
        let part = collect.get_key();
        if let Some(partition) = self.partitions.swap_remove(part) {
            let part_state = partition.snapshot().await;
            collect.add_state(ctx.operator_id, &part_state);
        }
    }

    async fn on_interrogate(
        &mut self,
        interrogate: &mut Interrogate<SrcImpl::PartitionKey>,
        output: &mut Output<(SrcImpl::PartitionKey, SrcImpl::Value, SrcImpl::Timestamp)>,
        ctx: &mut OperatorContext,
    ) {
        interrogate.add_keys(self.partitions.keys().cloned());
    }
}

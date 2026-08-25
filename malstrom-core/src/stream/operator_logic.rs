use std::{
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use crate::{
    channels::operator_io::{Input, Output, full_broadcast},
    snapshot::SnapshotBarrier,
    stream::{OperatorContext, WorkerBuildContext},
    types::distributed::{Acquire, Collect, Interrogate},
    types::{
        Barrier, Data, DataMessage, Kvt, MaybeKey, MaybeTime, Message, ReconfigComplete,
        RescaleMessage, SuspendMarker,
    },
};

use super::BuildContext;

pub trait LogicBuilder<M: Kvt, N: Kvt>: 'static {
    type Logic: Logic<M, N>;
    async fn build(self, ctx: &mut BuildContext) -> Self::Logic;
}

pub struct DirectLogic<L> {
    logic: L,
}

impl<L> DirectLogic<L> {
    pub(crate) fn new(logic: L) -> Self {
        Self { logic }
    }
}

impl<M, N, L> LogicBuilder<M, N> for DirectLogic<L>
where
    M: Kvt,
    N: Kvt,
    L: Logic<M, N> + 'static,
{
    type Logic = L;
    async fn build(self, _ctx: &mut BuildContext) -> Self::Logic {
        self.logic
    }
}

impl<M, N, F, L> LogicBuilder<M, N> for F
where
    F: AsyncFnOnce(&mut BuildContext) -> L + 'static,
    L: Logic<M, N>,
    M: Kvt,
    N: Kvt,
{
    type Logic = L;
    async fn build(self, ctx: &mut BuildContext) -> Self::Logic {
        (self)(ctx).await
    }
}

/// Operator Logic with absolutely no safeguard, allows you to break keying and everything else
///
/// # Example
/// ```
/// use malstrom_core::channels::operator_io::{Input, Output};
/// use malstrom_core::stream::{Logic, OperatorContext};
///
/// struct PassThrough;
/// impl Logic<(u64, u64, u64), (u64, u64, u64)> for PassThrough {
///     async fn apply(
///         &mut self,
///         input: &mut Input<(u64, u64, u64)>,
///         output: &mut Output<(u64, u64, u64)>,
///         _ctx: &mut OperatorContext,
///     ) {
///         let msg = input.recv().await;
///         output.send(msg).await;
///     }
/// }
/// ```
pub trait Logic<M: Kvt, N: Kvt>: 'static {
    async fn apply(
        &mut self,
        input: &mut Input<M>,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    );
}

impl<M, N, F> Logic<M, N> for F
where
    M: Kvt,
    N: Kvt,
    F: AsyncFnMut(&mut Input<M>, &mut Output<N>, &mut OperatorContext) + 'static,
{
    async fn apply(
        &mut self,
        input: &mut Input<M>,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
        self(input, output, ctx).await;
    }
}

/// This trait provides a way to implement logic with no risk of breaking internal messaging invariants.
/// Usually it does not make sense to implement this trait directly. Consider using
/// [malstrom::operators::StatefulLogic](StatefulLogic) instead.
///
/// # Example
/// ```
/// use malstrom_core::channels::operator_io::Output;
/// use malstrom_core::stream::{OperatorContext, SafeLogic};
/// use malstrom_core::types::{DataMessage, Message};
///
/// struct Doubler;
/// impl SafeLogic<(u64, u64, u64), (u64, u64, u64)> for Doubler {
///     async fn on_data(
///         &mut self,
///         data: DataMessage<(u64, u64, u64)>,
///         output: &mut Output<(u64, u64, u64)>,
///         _ctx: &mut OperatorContext,
///     ) {
///         output
///             .send(Message::Data(DataMessage::new(
///                 data.key,
///                 data.value * 2,
///                 data.timestamp,
///             )))
///             .await;
///     }
/// }
/// ```
pub trait SafeLogic<M: Kvt, N: Kvt<Key = M::Key>>: Sized + 'static {
    /// Called whenever this operator is scheduled by its worker.
    /// Return `true` if this call performed work (e.g. emitted messages) — the
    /// scheduler will keep calling until no more work remains.
    async fn on_schedule(&mut self, output: &mut Output<N>, ctx: &mut OperatorContext) -> bool {
        false
    }

    /// Called for every data message reaching the operator
    async fn on_data(
        &mut self,
        data_message: DataMessage<M>,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    );

    /// Called for every epoch reaching the operator
    async fn on_epoch(
        &mut self,
        epoch: &<M as Kvt>::Timestamp,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
    }

    /// Called for every snapshot barrier reaching the operator
    async fn on_barrier(
        &mut self,
        barrier: &mut Barrier,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
    }

    /// Called whenever a rescale message reaches the operator
    async fn on_rescale(
        &mut self,
        rescale_message: &mut RescaleMessage,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
    }

    /// Called when the SuspendMarker reaches the operator. This indicates the job will shutdown,
    /// even though execution is not finished.
    /// The operator will not be scheduled again after this until the job is restarted.
    async fn on_suspend(
        &mut self,
        suspend_marker: &mut SuspendMarker,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
    }

    /// Called when a key interrogation message reaches the operator.
    /// The operator must inform the interrogation message about all keys it currently
    /// holds in state
    async fn on_interrogate(
        &mut self,
        interrogate: &mut Interrogate<<M as Kvt>::Key>,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
    }

    /// Called when a key-state collection message reaches the operator.
    /// The operator must hand the state for the given key to the collection message.
    /// No more messages of the given key will reach the operator after this message
    async fn on_collect(
        &mut self,
        collect: &mut Collect<<M as Kvt>::Key>,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
    }

    /// Called when a key-state acquire message reaches the operator.
    /// The operator must take the state given by the acquire message and add it to its local key
    /// state.
    async fn on_acquire(
        &mut self,
        acquire: &mut Acquire<<M as Kvt>::Key>,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
    }

    async fn on_reconfig_complete(
        &mut self,
        reconfig_complete: &ReconfigComplete,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
    }

    /// Turn this type into a schedulable function which can be scheduled by the Malstrom worker.
    fn into_logic(self) -> SafeLogicWrapper<Self> {
        SafeLogicWrapper {
            implementation: self,
        }
    }
}

pub struct SafeLogicWrapper<L> {
    implementation: L,
}

impl<M, N, L> Logic<M, N> for SafeLogicWrapper<L>
where
    M: Kvt,
    N: Kvt<Key = M::Key, Timestamp = M::Timestamp>,
    L: SafeLogic<M, N>,
{
    async fn apply(
        &mut self,
        input: &mut Input<M>,
        output: &mut Output<N>,
        ctx: &mut OperatorContext,
    ) {
        // pump the schedule until it makes no progress, then handle one input message
        loop {
            if !self.implementation.on_schedule(output, ctx).await {
                break;
            }
        }
        match input.recv().await {
            Message::Data(data_message) => {
                self.implementation.on_data(data_message, output, ctx).await
            }
            Message::Epoch(epoch) => {
                self.implementation.on_epoch(&epoch, output, ctx).await;
                output.send(Message::Epoch(epoch)).await;
            }
            Message::AbsBarrier(mut barrier) => {
                self.implementation
                    .on_barrier(&mut barrier, output, ctx)
                    .await;
                output.send(Message::AbsBarrier(barrier)).await
            }
            Message::Rescale(mut rescale_message) => {
                self.implementation
                    .on_rescale(&mut rescale_message, output, ctx)
                    .await;
                output.send(Message::Rescale(rescale_message)).await
            }
            Message::Interrogate(mut interrogate) => {
                self.implementation
                    .on_interrogate(&mut interrogate, output, ctx)
                    .await;
                output.send(Message::Interrogate(interrogate)).await
            }
            Message::Collect(mut collect) => {
                self.implementation
                    .on_collect(&mut collect, output, ctx)
                    .await;
                output.send(Message::Collect(collect)).await
            }
            Message::Acquire(mut acquire) => {
                self.implementation
                    .on_acquire(&mut acquire, output, ctx)
                    .await;
                output.send(Message::Acquire(acquire)).await
            }
            Message::ReconfigComplete(reconfig_complete) => {
                self.implementation
                    .on_reconfig_complete(&reconfig_complete, output, ctx)
                    .await;
                output
                    .send(Message::ReconfigComplete(reconfig_complete))
                    .await
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::operator_io::{Input, Output, full_broadcast, link};
    use crate::types::{DataMessage, NoKey};

    /// A `SafeLogic` whose `on_schedule` emits one record per call while work remains.
    /// (Uses an `i32` timestamp: a `NoTime` output auto-closes after its first send —
    /// `CHECK_FINISHED` is always true for `NoTime` — which is not what this test is
    /// about.)
    struct EmitWhileWork {
        remaining: u32,
        events: flume::Sender<&'static str>,
    }

    type M = (NoKey, u32, i32);

    impl SafeLogic<M, M> for EmitWhileWork {
        async fn on_schedule(
            &mut self,
            output: &mut Output<M>,
            _ctx: &mut OperatorContext,
        ) -> bool {
            if self.remaining > 0 {
                self.remaining -= 1;
                self.events.send("emit").unwrap();
                output
                    .send(Message::Data(DataMessage::new(NoKey, self.remaining, 0)))
                    .await;
                true
            } else {
                false
            }
        }

        async fn on_data(
            &mut self,
            _data_message: DataMessage<M>,
            _output: &mut Output<M>,
            _ctx: &mut OperatorContext,
        ) {
        }
    }

    /// Regression: `on_schedule` is pumped until it returns false — a source with no
    /// input emits everything it has in a single apply, before any input message is
    /// handled.
    #[tokio::test]
    async fn on_schedule_pumps_until_idle() {
        let (tx_events, rx_events) = flume::unbounded();
        let mut logic = EmitWhileWork {
            remaining: 5,
            events: tx_events,
        }
        .into_logic();

        // pre-send one epoch so the apply's recv completes after the pump
        let mut input = Input::new_unlinked();
        let mut feeder: Output<M> = Output::new_unlinked(full_broadcast);
        link(&mut feeder, &mut input);
        feeder.send(Message::Epoch(0)).await;

        let mut output: Output<M> = Output::new_unlinked(full_broadcast);
        let mut collector = Input::new_unlinked();
        link(&mut output, &mut collector);

        logic
            .apply(&mut input, &mut output, &mut OperatorContext::new(0, 0))
            .await;

        // all five emissions happened before the input epoch was handled
        assert_eq!(rx_events.drain().collect::<Vec<_>>(), vec!["emit"; 5]);
        let mut datas = 0;
        for _ in 0..5 {
            if let Message::Data(d) = collector.recv().await {
                assert!(d.value < 5);
                datas += 1;
            }
        }
        assert_eq!(datas, 5);
        assert!(matches!(collector.recv().await, Message::Epoch(_)));
    }
}

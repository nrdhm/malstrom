//! Local IO channels for stream operators. These input and output types are how operators
//! **on the same worker** communicate with each other.
//! Essentially these are the edges in the stream graph.
use super::spsc;
use crate::{
    channels::{
        alignment::{AlignedValue, AlignmentGroup},
        recv_trait::Receiver,
        signal::{Signal, SignalHandle},
    },
    snapshot::SnapshotBarrier,
    types::{
        Barrier, Kvt, MaybeTime, Message, OperatorId, OperatorPartitioner, SuspendMarker, Timestamp,
    },
};
use futures::{FutureExt, StreamExt, TryFutureExt, stream::FuturesUnordered};
use itertools::Itertools;
use std::{rc::Rc, usize};
use tokio::sync::{oneshot, watch};

/// Operator Output
pub struct Output<M: Kvt> {
    // Each sender in this Vec is essentially one outgoing
    // edge from the operator. Wrapped in `Rc` so we can hand out
    // owned receiver-liveness futures without borrowing the Output.
    senders: Vec<Rc<spsc::Sender<Message<M>>>>,
    partitioner: Box<dyn OperatorPartitioner<M>>,
    frontier: Option<<M as Kvt>::Timestamp>,
    /// signal will be sent here if the Output gets closed,
    /// either because it has seen the MAX timestamp or because
    /// it was suspended
    closed_signal: watch::Sender<bool>,
}

impl<M: Kvt> Output<M> {
    /// Create a new Sender with **no** associated Receiver
    /// Link a receiver with [link].
    pub(crate) fn new_unlinked(partitioner: impl OperatorPartitioner<M>) -> Self {
        /// Allow NoTime type to indicate a final output
        /// even if send is never called on this output
        let finalized_signal = Signal::new(M::Timestamp::CHECK_FINISHED(&None));
        let this = Self {
            senders: Vec::new(),
            partitioner: Box::new(partitioner),
            frontier: None,
            closed_signal: watch::Sender::new(false),
        };
        this
    }

    /// Send a value into this channel.
    /// Data messages are distributed as per the partioning function.
    ///
    /// System messages are always broadcasted.
    pub async fn send(&mut self, msg: Message<M>)
    where
        M: Clone,
    {
        // a send may race with the output closing (e.g. an operator applying once more
        // during shutdown); dropping the message is fine at that point
        if *self.closed_signal.borrow() {
            return;
        }
        if let Message::Epoch(e) = &msg {
            if self.frontier.as_ref().is_some_and(|x| e > x) || self.frontier.is_none() {
                self.frontier = Some(e.clone());
            }
        }
        let recipient_len = self.senders.len();
        let mut output_flags = vec![false; recipient_len];
        match msg {
            Message::Data(x) => {
                (self.partitioner)(&x, &mut output_flags);
                let msg_count = output_flags.iter().map(|x| if *x { 1 } else { 0 }).sum();
                let mut messages = itertools::repeat_n(Message::Data(x), msg_count);
                for (enabled, sender) in output_flags.into_iter().zip_eq(self.senders.iter()) {
                    if enabled {
                        // PANIC: we know next will work because we called repeat_n with
                        // the sum of all `true` vals
                        #[allow(clippy::unwrap_used)]
                        let msg = messages.next().unwrap();
                        sender.send(msg).await;
                    }
                }
            }
            x => {
                if matches!(x, Message::AbsBarrier(Barrier::Suspend(_))) {
                    self.closed_signal.send(true);
                }
                // repeat_n will clone for every iteration except the last
                // this gives us a small optimization on the common "1 receiver" case :)
                let messages = self
                    .senders
                    .iter_mut()
                    .zip(itertools::repeat_n(x, recipient_len));
                for (sender, elem) in messages {
                    sender.send(elem).await;
                }
            }
        };
        if M::Timestamp::CHECK_FINISHED(&self.frontier) {
            self.closed_signal.send(true);
        };
    }
    /// Get the frontier on this Sender, i.e the timestamp of the largest
    /// Epoch sent with this sender or `None` if no Epoch has been sent with
    /// this sender yet
    #[inline]
    pub fn get_frontier(&self) -> &Option<<M as Kvt>::Timestamp> {
        &self.frontier
    }

    pub(crate) fn get_closed_signal(&self) -> ClosedSignal {
        let sub = self.closed_signal.subscribe();
        ClosedSignal(sub)
    }

    /// Mark this output as closed, causing downstream operators watching
    /// [get_closed_signal] to stop.
    pub(crate) fn close(&self) {
        let _ = self.closed_signal.send(true);
    }

    /// Resolves once all receivers of this output's channels are gone, i.e. the
    /// downstream operator(s) terminated. Only meaningful for outputs which
    /// actually have senders (an unlinked output, like a sink's, never resolves).
    /// Returns an owned future so it can be polled alongside `&mut` borrows of
    /// the output.
    pub(crate) fn no_receivers(&self) -> impl Future<Output = ()> + 'static {
        if self.senders.is_empty() {
            return futures::future::pending().boxed_local();
        }
        futures::future::join_all(self.senders.iter().cloned().map(|sender| async move {
            sender.wait_receiver_gone().await;
        }))
        .map(|_| ())
        .boxed_local()
    }
}

pub(crate) struct ClosedSignal(watch::Receiver<bool>);

impl ClosedSignal {
    pub(crate) fn wait_for(&mut self) -> impl Future<Output = ()> + '_ {
        // can ignore result because Err just means Sender was dropped
        async move {
            let _ = self.0.wait_for(|x| *x).await;
        }
    }
}

#[derive(Default)]
pub(crate) struct RootOutput {
    senders: Vec<spsc::Sender<Message<()>>>,
}

impl RootOutput {
    // send a system message, this method is not async to allow sending
    // from a different or no runtime
    pub(crate) fn send_system(&mut self, msg: Message<()>) {
        for s in self.senders.iter() {
            s.force_send(msg.clone())
        }
    }
}

/// State of the upstream sender providing us messages
#[derive(Default)]
struct UpstreamState<M: Kvt> {
    /// Most recent epoch the sender sent
    epoch: Option<M::Timestamp>,
    /// Barrier currently waiting for alignment
    barred: bool,
    /// Susepend currently waiting for alignment
    suspended: bool,
}
impl<M: Kvt> UpstreamState<M> {
    fn new() -> Self {
        Self {
            epoch: None,
            barred: false,
            suspended: false,
        }
    }
}

/// Outer group for Barriers, inner group for SuspendMarkers
type BarrierAlign<M> =
    AlignmentGroup<OperatorId, spsc::Receiver<Message<M>>, fn(&Message<M>) -> bool>;

fn is_barrier<M: Kvt>(msg: &Message<M>) -> bool {
    matches!(msg, Message::AbsBarrier(_))
}

/// Operator Input
pub struct Input<M: Kvt> {
    /// Highest epoch seen so far per inbound edge,
    frontiers: Vec<Option<M::Timestamp>>,
    /// last Epoch value we sent out
    last_epoch: Option<M::Timestamp>,
    receivers: BarrierAlign<M>,
}

impl<M: Kvt> Input<M> {
    /// Create a new input which is not (yet) linked to any output
    pub(crate) fn new_unlinked() -> Input<M> {
        let barrier_align = BarrierAlign::new_empty(is_barrier);
        Self {
            frontiers: Vec::new(),
            last_epoch: None,
            receivers: barrier_align,
        }
    }
}

impl<M: Kvt> Input<M>
where
    M::Timestamp: MaybeTime,
{
    /// Get the frontier of this Input, i.e. the smallest Epoch currently merged
    #[inline]
    pub(crate) fn get_frontier(&self) -> Option<M::Timestamp> {
        self.last_epoch.clone()
    }

    /// Non-blocking receive: returns a message if one is immediately available.
    /// Polls with a no-op waker, so any registered waker on an empty channel may
    /// be overwritten — only use where no other task waits on this input
    /// (e.g. the single-threaded [crate::testing::OperatorTester]).
    pub(crate) fn try_recv(&mut self) -> Option<Message<M>> {
        let mut fut = std::pin::pin!(self.receivers.recv());
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(&waker);
        match fut.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(AlignedValue::Unaligned((_, msg))) => Some(msg),
            std::task::Poll::Ready(AlignedValue::Aligned(mut items)) => {
                items.pop().map(|(_, msg)| msg)
            }
            std::task::Poll::Pending => None,
        }
    }
}

impl<M: Kvt> Input<M>
where
    <M as Kvt>::Timestamp: MaybeTime,
{
    /// Receive a value
    ///
    /// This method synchronizes barriers, i.e. if a channel is barred, it will
    /// not receive any messages from that channel until all channels are barred.
    /// Once all channels are barred, a single barrier will be emitted
    pub async fn recv(&mut self) -> Message<M> {
        loop {
            // We loop here just for the case where we get an epoch but can not emit it
            // because of inputs which are behind or because it would not advance the frontier
            let (key, msg) = match self.receivers.recv().await {
                AlignedValue::Unaligned((key, msg)) => (key, msg),
                AlignedValue::Aligned(mut items) => {
                    // does not matter which barrier we send, as long as they are aligned
                    // index also does not matter
                    items
                        .pop()
                        .expect("Expected at least one receiver in Input")
                }
            };
            match msg {
                Message::Epoch(e) => {
                    self.frontiers[key as usize] = Some(e);
                    let merged = merge_timestamps(self.frontiers.iter());
                    // Only sent out if we would advance the frontier
                    // TODO: test
                    let out_epoch = match (self.last_epoch.as_ref(), merged) {
                        (None, Some(e)) => Some(e),
                        (Some(le), Some(me)) if me > *le => Some(me),
                        _ => None,
                    };
                    if let Some(e) = out_epoch {
                        self.last_epoch = Some(e.clone());
                        return Message::Epoch(e);
                    }
                }
                x => return x,
            }
        }
    }
}

// /// A simple partitioner, which will broadcast a value to all receivers
#[inline(always)]
pub(crate) fn full_broadcast<T>(_: &T, outputs: &mut [bool]) {
    outputs.fill(true);
}

/// Link a Sender and receiver together
pub(crate) fn link<M: Kvt>(sender: &mut Output<M>, receiver: &mut Input<M>) {
    let (tx, rx) = spsc::unbounded();
    sender.senders.push(Rc::new(tx));
    // receiver keys are 0-based so they line up with `frontiers`
    let next_key = receiver.receivers.keys().last().map_or(0, |k| k + 1);
    receiver.receivers.insert(next_key, rx);
    receiver.frontiers.push(None);
}

/// Small reducer hack, as we can't use iter::reduce because of ownership
/// TODO: Move this somewhere else
pub(crate) fn merge_timestamps<'a, T: MaybeTime>(
    mut timestamps: impl Iterator<Item = &'a Option<T>>,
) -> Option<T> {
    let mut merged = timestamps.next()?.clone();
    for x in timestamps {
        if let Some(y) = x {
            merged = merged.and_then(|a| a.try_merge(y));
        } else {
            return None;
        }
    }
    merged
}

#[cfg(test)]
mod test {
    use crate::{
        snapshot::{NoPersistence, SnapshotBarrier},
        types::{Barrier, DataMessage, NoData, NoKey, NoTime},
    };

    use super::*;

    /// receive, returning `None` if nothing arrives within a short timeout
    async fn recv_or_none<M: Kvt>(input: &mut Input<M>) -> Option<Message<M>> {
        tokio::time::timeout(std::time::Duration::from_millis(10), input.recv())
            .await
            .ok()
    }

    /// Check we only emit an epoch when it changes
    #[tokio::test]
    async fn emit_epoch_on_change() {
        let mut sender: Output<(NoKey, NoData, i32)> = Output::new_unlinked(full_broadcast);
        let mut sender2: Output<(NoKey, NoData, i32)> = Output::new_unlinked(full_broadcast);
        let mut receiver = Input::new_unlinked();
        link(&mut sender, &mut receiver);
        link(&mut sender2, &mut receiver);

        sender.send(Message::Epoch(42)).await;

        assert!(recv_or_none(&mut receiver).await.is_none());
        sender2.send(Message::Epoch(15)).await;
        assert!(matches!(
            recv_or_none(&mut receiver).await,
            Some(Message::Epoch(15))
        ));
    }

    /// only issue a barrier once it is aligned
    #[tokio::test]
    async fn aligns_barriers() {
        let mut sender: Output<(NoKey, NoData, i32)> = Output::new_unlinked(full_broadcast);
        let mut sender2: Output<(NoKey, NoData, i32)> = Output::new_unlinked(full_broadcast);
        let mut receiver = Input::new_unlinked();
        link(&mut sender, &mut receiver);
        link(&mut sender2, &mut receiver);

        let (cb, _rx) = tokio::sync::mpsc::channel(1);
        sender
            .send(Message::AbsBarrier(Barrier::Snapshot(
                SnapshotBarrier::new(Box::new(NoPersistence), cb),
            )))
            .await;

        let received = recv_or_none(&mut receiver).await;
        assert!(received.is_none());
        let (cb, _rx) = tokio::sync::mpsc::channel(1);
        sender2
            .send(Message::AbsBarrier(Barrier::Snapshot(
                SnapshotBarrier::new(Box::new(NoPersistence), cb),
            )))
            .await;

        assert!(matches!(
            recv_or_none(&mut receiver).await,
            Some(Message::AbsBarrier(_))
        ));
    }

    /// should buffer messages if the channels if barred
    #[tokio::test]
    async fn buffer_on_barriers() {
        let mut sender: Output<(NoKey, i32, NoTime)> = Output::new_unlinked(full_broadcast);
        let mut sender2: Output<(NoKey, i32, NoTime)> = Output::new_unlinked(full_broadcast);
        let mut receiver = Input::new_unlinked();
        link(&mut sender, &mut receiver);
        link(&mut sender2, &mut receiver);

        let (cb, _rx) = tokio::sync::mpsc::channel(1);
        sender
            .send(Message::AbsBarrier(Barrier::Snapshot(
                SnapshotBarrier::new(Box::new(NoPersistence), cb),
            )))
            .await;

        sender
            .send(Message::Data(DataMessage::new(NoKey, 42, NoTime)))
            .await;
        sender
            .send(Message::Data(DataMessage::new(NoKey, 177, NoTime)))
            .await;

        let (cb, _rx) = tokio::sync::mpsc::channel(1);
        sender2
            .send(Message::AbsBarrier(Barrier::Snapshot(
                SnapshotBarrier::new(Box::new(NoPersistence), cb),
            )))
            .await;
        assert!(matches!(
            recv_or_none(&mut receiver).await,
            Some(Message::AbsBarrier(_))
        ));

        let msg = recv_or_none(&mut receiver).await;
        assert!(matches!(
            msg,
            Some(Message::Data(DataMessage {
                key: _,
                value: 42,
                timestamp: _
            }))
        ));
        assert!(matches!(
            recv_or_none(&mut receiver).await,
            Some(Message::Data(DataMessage {
                key: _,
                value: 177,
                timestamp: _
            }))
        ));
    }

    /// Check the accessor for the largest sent epoch (frontier)
    #[tokio::test]
    async fn observe_frontier() {
        let mut sender: Output<(NoKey, NoData, i32)> = Output::new_unlinked(full_broadcast);
        let mut receiver = Input::new_unlinked();
        link(&mut sender, &mut receiver);

        assert_eq!(*sender.get_frontier(), None);
        // non-epoch messages should not influence this
        sender
            .send(Message::Data(DataMessage::new(NoKey, NoData, 1337)))
            .await;
        assert_eq!(*sender.get_frontier(), None);

        sender.send(Message::Epoch(42)).await;
        assert_eq!(*sender.get_frontier(), Some(42));
        sender.send(Message::Epoch(15)).await;
        assert_eq!(*sender.get_frontier(), Some(42));
        sender.send(Message::Epoch(i32::MAX)).await;
        assert_eq!(*sender.get_frontier(), Some(i32::MAX));
    }

    #[tokio::test]
    async fn receiver_observe_frontier() {
        let mut sender1: Output<(NoKey, NoData, i32)> = Output::new_unlinked(full_broadcast);
        let mut sender2: Output<(NoKey, NoData, i32)> = Output::new_unlinked(full_broadcast);
        let mut receiver = Input::new_unlinked();
        link(&mut sender1, &mut receiver);
        link(&mut sender2, &mut receiver);

        sender1.send(Message::Epoch(42)).await;
        // not yet aligned
        let _ = recv_or_none(&mut receiver).await;
        assert_eq!(receiver.get_frontier(), None);

        sender2.send(Message::Epoch(78)).await;
        let _ = recv_or_none(&mut receiver).await;
        assert_eq!(receiver.get_frontier(), Some(42));

        sender1.send(Message::Epoch(1337)).await;
        sender2.send(Message::Epoch(1337)).await;
        let _ = recv_or_none(&mut receiver).await;
        let _ = recv_or_none(&mut receiver).await;
        assert_eq!(receiver.get_frontier(), Some(1337));
    }

    #[test]
    fn merges_timestamps() {
        assert_eq!(merge_timestamps([None, Some(43)].iter()), None);
        assert_eq!(merge_timestamps([Some(42), Some(43)].iter()), Some(42));
        assert_eq!(
            merge_timestamps([Some(1337), Some(1337)].iter()),
            Some(1337)
        );
        assert_eq!(merge_timestamps::<i32>([None, None].iter()), None);
    }

    /// Should just discard messages
    #[tokio::test]
    async fn sender_without_sink_discards() {
        let mut sender: Output<(&str, Rc<&str>, i32)> = Output::new_unlinked(full_broadcast);
        let elem = Rc::new("brox");
        // this should not panic
        sender
            .send(Message::Data(DataMessage::new("Beeble", elem.clone(), 42)))
            .await;
        // if the sender had kept or sent the message somewhere this should panic
        Rc::try_unwrap(elem).unwrap();
    }
}

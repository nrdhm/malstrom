//! Contract test for the kernel's `SafeLogic` extension seam, exercised from the
//! outside (this file imports only `malstrom_core`, never the operator/distributed
//! crates).
//!
//! Pins the wrapper's dispatch contract: `on_schedule` is pumped before every
//! received message, the typed handlers run in message order, and system messages
//! (`Epoch`, `AbsBarrier`) are forwarded downstream automatically after the handler.

use malstrom_core::{
    channels::operator_io::{Input, Output},
    runtime::SingleThreadRuntime,
    snapshot::{NoPersistence, SnapshotBarrier},
    stream::{BuildContext, Logic, LogicBuilder, Malstrom as _, Operator, OperatorContext, SafeLogic},
    types::{Barrier, DataMessage, Kvt, Message},
    worker::StreamProvider,
};

type Msg = (usize, usize, usize);

/// A kernel-only source emitting `Data(0)`, an `AbsBarrier`, `Data(1)`, then
/// `Epoch(MAX)` — once, on the first apply.
struct Source {
    sent: bool,
}

impl Logic<(), Msg> for Source {
    async fn apply(
        &mut self,
        _input: &mut Input<()>,
        output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        if !self.sent {
            self.sent = true;
            output.send(Message::Data(DataMessage::new(0, 0, 0))).await;
            let (cb_tx, _cb_rx) = tokio::sync::mpsc::channel(1);
            output
                .send(Message::AbsBarrier(Barrier::Snapshot(SnapshotBarrier::new(
                    Box::new(NoPersistence),
                    cb_tx,
                ))))
                .await;
            output.send(Message::Data(DataMessage::new(1, 1, 1))).await;
            output.send(Message::Epoch(usize::MAX)).await;
        }
    }
}

/// A `SafeLogic` operator that records the dispatch order and forwards everything.
struct Recorder {
    events: flume::Sender<&'static str>,
}

impl SafeLogic<Msg, Msg> for Recorder {
    async fn on_schedule(
        &mut self,
        _output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) -> bool {
        self.events.send("schedule").unwrap();
        false
    }

    async fn on_data(
        &mut self,
        data_message: DataMessage<Msg>,
        output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        self.events.send("data").unwrap();
        output.send(Message::Data(data_message)).await;
    }

    async fn on_epoch(
        &mut self,
        _epoch: &<Msg as Kvt>::Timestamp,
        _output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        self.events.send("epoch").unwrap();
    }

    async fn on_barrier(
        &mut self,
        _barrier: &mut Barrier,
        _output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        self.events.send("barrier").unwrap();
    }
}

/// A raw-`Logic` terminal that records what it receives and forwards everything (so its
/// output closes on `Epoch(MAX)` and the job terminates).
struct Sink {
    seen: flume::Sender<&'static str>,
}

impl Logic<Msg, Msg> for Sink {
    async fn apply(
        &mut self,
        input: &mut Input<Msg>,
        output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        match input.recv().await {
            Message::Data(data_message) => {
                self.seen.send("sink-data").unwrap();
                output.send(Message::Data(data_message)).await;
            }
            Message::Epoch(epoch) => {
                self.seen.send("sink-epoch").unwrap();
                output.send(Message::Epoch(epoch)).await;
            }
            Message::AbsBarrier(barrier) => {
                self.seen.send("sink-barrier").unwrap();
                output.send(Message::AbsBarrier(barrier)).await;
            }
            other => output.send(other).await,
        }
    }
}

#[test]
fn safe_logic_dispatch_order_and_system_message_forwarding() {
    let (tx_events, rx_events) = flume::unbounded();
    let (tx_seen, rx_seen) = flume::unbounded();

    SingleThreadRuntime::builder()
        .persistence(NoPersistence)
        .build(move |provider: &mut dyn StreamProvider| {
            let tx_events = tx_events.clone();
            let tx_seen = tx_seen.clone();
            provider
                .new_stream()
                .then(Operator::built_by(
                    "source".to_string(),
                    |_ctx: &mut BuildContext| async { Source { sent: false } },
                ))
                .then(Operator::built_by(
                    "recorder".to_string(),
                    move |_ctx: &mut BuildContext| async move {
                        Recorder { events: tx_events }.into_logic()
                    },
                ))
                .then(Operator::built_by(
                    "sink".to_string(),
                    move |_ctx: &mut BuildContext| async move { Sink { seen: tx_seen } },
                ));
        })
        .execute()
        .unwrap();

    // one on_schedule pump before each of the four messages, in order
    assert_eq!(
        rx_events.drain().collect::<Vec<_>>(),
        vec![
            "schedule", "data", "schedule", "barrier", "schedule", "data", "schedule", "epoch",
        ],
        "on_schedule must be pumped before every message, in source order"
    );
    // the barrier and the epoch are forwarded downstream by the wrapper
    assert_eq!(
        rx_seen.drain().collect::<Vec<_>>(),
        vec!["sink-data", "sink-barrier", "sink-data", "sink-epoch"],
        "data, barrier and epoch must all reach the terminal operator"
    );
}

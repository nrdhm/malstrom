//! Completion contract: a kernel-only source emitting `Epoch(MAX)` must terminate the
//! runtime — for both runtimes — and every record emitted before MAX must be seen. This
//! pins the root/no-receivers/completion protocol from the outside.

mod common;

use common::init_logs;
use malstrom_core::{
    channels::operator_io::{Input, Output},
    runtime::{MultiThreadRuntime, SingleThreadRuntime},
    snapshot::NoPersistence,
    stream::{BuildContext, Logic, LogicBuilder, Malstrom as _, Operator, OperatorContext},
    types::{DataMessage, Message},
    worker::StreamProvider,
};

type Msg = (usize, usize, usize);

/// Emits `0..max` then `Epoch(MAX)`, once.
struct Numbers {
    next: usize,
    max: usize,
    done: bool,
}

impl Logic<(), Msg> for Numbers {
    async fn apply(
        &mut self,
        _input: &mut Input<()>,
        output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        if !self.done {
            self.done = true;
            while self.next < self.max {
                output
                    .send(Message::Data(DataMessage::new(
                        self.next, self.next, self.next,
                    )))
                    .await;
                self.next += 1;
            }
            output.send(Message::Epoch(usize::MAX)).await;
        }
    }
}

/// A pass-through operator that collects the values it sees.
struct Collect {
    values: flume::Sender<usize>,
}

impl Logic<Msg, Msg> for Collect {
    async fn apply(
        &mut self,
        input: &mut Input<Msg>,
        output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        let msg = input.recv().await;
        if let Message::Data(d) = &msg {
            self.values.send(d.value).unwrap();
        }
        output.send(msg).await;
    }
}

fn build_dataflow(provider: &mut dyn StreamProvider, tx: flume::Sender<usize>) {
    let tx = tx.clone();
    provider
        .new_stream()
        .then(Operator::built_by(
            "numbers".to_string(),
            |_ctx: &mut BuildContext| async {
                Numbers {
                    next: 0,
                    max: 5,
                    done: false,
                }
            },
        ))
        .then(Operator::built_by(
            "collect".to_string(),
            move |_ctx: &mut BuildContext| async move { Collect { values: tx } },
        ));
}

fn assert_seen(rx: flume::Receiver<usize>, expected: Vec<usize>) {
    let mut values: Vec<usize> = rx.drain().collect();
    values.sort_unstable();
    assert_eq!(values, expected);
}

#[test]
fn single_thread_runtime_terminates_on_max_epoch() {
    init_logs();
    let (tx, rx) = flume::unbounded();
    SingleThreadRuntime::builder()
        .persistence(NoPersistence)
        .build(move |provider: &mut dyn StreamProvider| build_dataflow(provider, tx))
        .execute()
        .unwrap();
    assert_seen(rx, vec![0, 1, 2, 3, 4]);
}

#[test]
fn multi_thread_runtime_terminates_on_max_epoch() {
    init_logs();
    let (tx, rx) = flume::unbounded();
    MultiThreadRuntime::builder()
        .parrallelism(2)
        .persistence(NoPersistence)
        .build(move |provider: &mut dyn StreamProvider| build_dataflow(provider, tx.clone()))
        .execute()
        .unwrap();
    // two workers, each emitting 0..5
    assert_seen(rx, vec![0, 0, 1, 1, 2, 2, 3, 3, 4, 4]);
}

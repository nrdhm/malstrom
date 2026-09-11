//! Rescale contract: `MultiThreadRuntime::api_handle().rescale(n)` scales up without
//! deadlock and the job still completes. Deterministic — completion is gated on a
//! shared flag, not on sleeps.
//!
//! Regression (three kernel bugs found while writing this test):
//! 1. `ClusterHandle::reconfigure` sent the startup protocol (`StartBuild` /
//!    `StartExecution`, tuple structs) to *existing* workers, whose coordination tasks
//!    only decode the `RuntimeMessage` enum — the bytes were mis-decoded as the enum
//!    and panicked, killing the coordinator loop. Only newly-added workers may be
//!    bootstrapped; existing workers learn the new scale via `RuntimeMessage::Reconfigure`.
//! 2. `MultiThreadRuntime::execute` counted the coordinator thread in `threads.len()`
//!    when deciding whether to spawn workers for a rescale, so a rescale from P to
//!    P+1 workers never spawned the new worker.
//! 3. The spsc channel queued messages even after its receiver was dropped; a terminal
//!    operator's output (whose tail receiver is dropped at build time) filled the
//!    bounded queue and then blocked forever, stalling the whole pipeline before a
//!    rescale could be processed.

mod common;

use common::init_logs;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use malstrom_core::{
    channels::operator_io::{Input, Output},
    runtime::MultiThreadRuntime,
    snapshot::NoPersistence,
    stream::{BuildContext, Logic, LogicBuilder, Malstrom as _, Operator, OperatorContext},
    types::{DataMessage, Message},
    worker::StreamProvider,
};

type Msg = (usize, usize, usize);

/// Emits one record per schedule. Once the test sets the finish flag, emits
/// `Epoch(MAX)` (once) and the stream (and job) completes.
///
/// System messages on the input (`Rescale`, barriers, …) are forwarded downstream,
/// mirroring how the operators-layer `Source` drives itself — required so the
/// coordinator's rescale handshake can complete (the handshake acks when the
/// `RescaleMessage` reaches the end of the pipeline and is dropped).
struct GatedSource {
    tx_seen: flume::Sender<usize>,
    finish: Arc<AtomicBool>,
    finished: bool,
}

impl Logic<(), Msg> for GatedSource {
    async fn apply(
        &mut self,
        input: &mut Input<()>,
        output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        // prefer forwarding any pending system message
        if let Some(msg) = input.try_recv() {
            match msg {
                Message::Data(_) => (),
                Message::Epoch(_) => (),
                Message::AbsBarrier(x) => output.send(Message::AbsBarrier(x)).await,
                Message::Rescale(x) => output.send(Message::Rescale(x)).await,
                Message::ReconfigComplete(x) => output.send(Message::ReconfigComplete(x)).await,
                Message::Interrogate(_) => (),
                Message::Collect(_) => (),
                Message::Acquire(_) => (),
            }
            return;
        }
        tokio::select! {
            biased;
            msg = input.recv() => match msg {
                Message::Data(_) => (),
                Message::Epoch(_) => (),
                Message::AbsBarrier(x) => output.send(Message::AbsBarrier(x)).await,
                Message::Rescale(x) => output.send(Message::Rescale(x)).await,
                Message::ReconfigComplete(x) => output.send(Message::ReconfigComplete(x)).await,
                Message::Interrogate(_) => (),
                Message::Collect(_) => (),
                Message::Acquire(_) => (),
            },
            _ = async { self.tx_seen.send(0) } => {
                if !self.finished {
                    if self.finish.load(Ordering::Relaxed) {
                        self.finished = true;
                        output.send(Message::Epoch(usize::MAX)).await;
                    } else {
                        output.send(Message::Data(DataMessage::new(0, 0, 0))).await;
                    }
                }
            }
        }
    }
}

/// Pass-through terminal (closes on `Epoch(MAX)`).
struct Forward;

impl Logic<Msg, Msg> for Forward {
    async fn apply(
        &mut self,
        input: &mut Input<Msg>,
        output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        let msg = input.recv().await;
        output.send(msg).await;
    }
}

#[test]
fn rescale_scales_up_without_deadlock_and_job_completes() {
    init_logs();
    let (tx_seen, rx_seen) = flume::unbounded();
    let finish = Arc::new(AtomicBool::new(false));
    let finish_test = Arc::clone(&finish);

    let rt = MultiThreadRuntime::builder()
        .parrallelism(1)
        .persistence(NoPersistence)
        .build(move |provider: &mut dyn StreamProvider| {
            let tx_seen = tx_seen.clone();
            let finish = Arc::clone(&finish);
            provider
                .new_stream()
                .then(Operator::built_by(
                    "source".to_string(),
                    move |_ctx: &mut BuildContext| async move {
                        GatedSource {
                            tx_seen,
                            finish,
                            finished: false,
                        }
                    },
                ))
                .then(Operator::built_by(
                    "forward".to_string(),
                    |_ctx: &mut BuildContext| async { Forward },
                ));
        });
    let api = rt.api_handle();
    let handle = std::thread::spawn(move || rt.execute().unwrap());

    // wait until the source has emitted (deterministic: no sleeps)
    rx_seen.recv().unwrap();

    // rescale 1 -> 2 from a tokio runtime, as a real caller would
    let tokio_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio_rt.block_on(api.rescale(2)).unwrap();

    // release the sources; the job must terminate (both workers finish)
    finish_test.store(true, Ordering::Relaxed);
    handle.join().expect("job must terminate after rescale");
}

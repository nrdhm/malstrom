use std::{
    collections::{HashMap, VecDeque},
    marker::PhantomData,
    ops::Range,
    rc::Rc,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;

use crate::{
    channels::operator_io::{Input, Output, full_broadcast, link},
    runtime::{
        OperatorOperatorComm,
        communication::{StreamReceiver, StreamSender},
    },
    snapshot::NoPersistence,
    stream::{BuildContext, Logic, LogicBuilder, OperatorContext},
    types::{Kvt, Message, OperatorId, WorkerId, distributable::Distributable},
};

/// A test harness for a single operator's logic, decoupled from a running worker.
pub struct OperatorTester<In: Kvt, Out: Kvt, L, R> {
    logic: L,
    input: Input<In>,
    input_handle: Output<In>,

    output: Output<Out>,
    output_handle: Input<Out>,
    comm_shim: Rc<FakeCommunication<R>>,

    worker_id: WorkerId,
    operator_id: OperatorId,
}

impl<In, Out, L, R> OperatorTester<In, Out, L, R>
where
    In: Kvt,
    Out: Kvt,
    L: Logic<In, Out>,
    R: Distributable + Send + Sync + 'static,
{
    /// Build this Test from an operator builder function
    pub(crate) async fn built_by(
        logic_builder: impl LogicBuilder<In, Out, Logic = L>,
        worker_id: WorkerId,
        operator_id: OperatorId,
        worker_ids: Range<u64>,
    ) -> Self {
        assert!(worker_ids.contains(&worker_id));
        let mut input_handle = Output::new_unlinked(full_broadcast);
        let mut input = Input::new_unlinked();
        link(&mut input_handle, &mut input);

        let mut output = Output::new_unlinked(full_broadcast);
        let mut output_handle = Input::new_unlinked();
        link(&mut output, &mut output_handle);

        let comm_shim = Rc::new(FakeCommunication::default());
        let rt =
            Rc::new(tokio::runtime::LocalRuntime::new().expect("Failed to create LocalRuntime"));
        let mut build_ctx = BuildContext::new(
            worker_id,
            operator_id,
            Rc::clone(&rt),
            "test".to_owned(),
            0,
            Rc::new(NoPersistence) as Rc<dyn crate::snapshot::PersistenceClient>,
            Rc::clone(&comm_shim) as Rc<dyn OperatorOperatorComm>,
            worker_ids.collect(),
        );
        let logic = logic_builder.build(&mut build_ctx).await;

        // The runtime must not be dropped inside the async test context (tokio panics
        // on that). LocalRuntime spawns no threads on its own, so leaking it is fine
        // for unit tests.
        std::mem::forget(rt);

        Self {
            logic,
            input,
            input_handle,
            output,
            output_handle,
            comm_shim,
            worker_id,
            operator_id,
        }
    }

    /// Send a message to the operators local input
    pub fn send_local(&mut self, msg: Message<In>) {
        futures::executor::block_on(self.input_handle.send(msg));
    }

    /// Receive a message from this operators local output, if one is immediately
    /// available. Returns `None` when the output is empty.
    pub fn recv_local(&mut self) -> Option<Message<Out>> {
        self.output_handle.try_recv()
    }

    /// Get a fake commounication backend to emulate remote communication
    /// on this operator
    pub fn remote(&self) -> &FakeCommunication<R> {
        &self.comm_shim
    }

    /// Perform one execution step on the operator
    pub fn step(&mut self) {
        let mut op_ctx = OperatorContext::new(self.worker_id, self.operator_id);
        futures::executor::block_on(self.logic.apply(
            &mut self.input,
            &mut self.output,
            &mut op_ctx,
        ));
    }
}

/// This is a Fake communication backend we can use in unit tests to emulate cross-worker
/// Communication
pub struct FakeCommunication<R> {
    // these are the messages the operator under test sent
    sent_by_operator: Arc<Mutex<VecDeque<SentMessage<R>>>>,
    // these are the messages the operator under test is yet to receive
    sent_to_operator: Arc<Mutex<HashMap<ImpersonatedSender, VecDeque<R>>>>,
}

impl<R> Default for FakeCommunication<R> {
    fn default() -> Self {
        Self {
            sent_by_operator: Default::default(),
            sent_to_operator: Default::default(),
        }
    }
}

impl<R> FakeCommunication<R> {
    /// Send a message to the operator under test, pretending to be a given worker and operator
    ///
    /// # Example
    /// ```
    /// use malstrom::testing::FakeCommunication;
    /// let comm = FakeCommunication::<String>::default();
    /// // pretend operator `4` on worker `15` sends the message `"Hello World"`
    /// comm.send_to_operator("Hello World".to_owned(), 15, 4);
    /// ```
    pub fn send_to_operator(&self, msg: R, from_worker: WorkerId, from_operator: OperatorId) {
        let key = ImpersonatedSender {
            worker_id: from_worker,
            operator_id: from_operator,
        };
        let mut guard = self.sent_to_operator.lock().unwrap();
        guard.entry(key).or_default().push_back(msg);
    }

    /// Receive a message sent from the operator under test.
    /// None if there are no non-received messages from the operator
    pub fn recv_from_operator(&self) -> Option<SentMessage<R>> {
        self.sent_by_operator.lock().unwrap().pop_front()
    }
}

/// A Message the operator under test has sent
#[derive(Debug)]
pub struct SentMessage<R> {
    /// WorkerId this message was intended for
    pub to_worker: WorkerId,
    /// OperatorId this message was intended for
    pub to_operator: OperatorId,
    /// Message content
    pub msg: R,
}

/// This is the sender we impersonate when we send a message to the operator under test
#[derive(Debug, Hash, PartialEq, Eq)]
struct ImpersonatedSender {
    worker_id: WorkerId,
    operator_id: OperatorId,
}

#[async_trait]
impl<R> OperatorOperatorComm for FakeCommunication<R>
where
    R: Distributable + Send + Sync + 'static,
{
    async fn new_sender(
        &self,
        to_worker: WorkerId,
        channel_id: OperatorId,
    ) -> Result<Box<dyn StreamSender>, Box<dyn std::error::Error>> {
        Ok(Box::new(FakeCommSender {
            sent_by_operator: Arc::clone(&self.sent_by_operator),
            to_worker,
            to_operator: channel_id,
            _phantom: PhantomData,
        }))
    }

    async fn new_receiver(
        &self,
        from_worker: WorkerId,
        channel_id: OperatorId,
    ) -> Result<Box<dyn StreamReceiver>, Box<dyn std::error::Error>> {
        Ok(Box::new(FakeCommReceiver {
            sent_to_operator: Arc::clone(&self.sent_to_operator),
            from_worker,
            from_operator: channel_id,
            _phantom: PhantomData,
        }))
    }
}

struct FakeCommSender<R> {
    // these are the messages the operator under test sent
    sent_by_operator: Arc<Mutex<VecDeque<SentMessage<R>>>>,
    to_worker: WorkerId,
    to_operator: OperatorId,
    _phantom: PhantomData<R>,
}

#[async_trait]
impl<R> StreamSender for FakeCommSender<R>
where
    R: Distributable + Send + Sync,
{
    async fn send(&self, msg: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        let decoded: R = R::decode(&msg);
        self.sent_by_operator
            .lock()
            .unwrap()
            .push_back(SentMessage {
                to_worker: self.to_worker,
                to_operator: self.to_operator,
                msg: decoded,
            });
        Ok(())
    }
}

struct FakeCommReceiver<R> {
    // these are the messages the operator under test is yet to receive
    sent_to_operator: Arc<Mutex<HashMap<ImpersonatedSender, VecDeque<R>>>>,
    from_worker: WorkerId,
    from_operator: OperatorId,
    _phantom: PhantomData<R>,
}

#[async_trait]
impl<R> StreamReceiver for FakeCommReceiver<R>
where
    R: Distributable + Send + Sync,
{
    async fn recv(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut guard = self.sent_to_operator.lock().unwrap();
        let key = ImpersonatedSender {
            worker_id: self.from_worker,
            operator_id: self.from_operator,
        };
        let msg = guard.get_mut(&key).and_then(|q| q.pop_front());
        Ok(msg.map(R::encode).unwrap_or_default())
    }
}

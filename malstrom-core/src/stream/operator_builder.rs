//! A builder to build JetStream operators

use crate::{
    channels::operator_io::{Input, Output, full_broadcast},
    stream::{DirectLogic, Logic, LogicBuilder, Operator},
    types::Kvt,
};

/// A convenient way to define an operator.
///
/// Implementation detail: the plumbing combinators (union/split) use to build edges
/// without touching `Operator` fields. Not part of the user-facing extension API; kept
/// `pub` for `malstrom-operators`, hidden from docs. See the public-API surface audit
/// (`docs/overviews/08-public-api-surface.md`).
#[doc(hidden)]
pub struct OperatorBuilder<M: Kvt, B, N: Kvt> {
    name: String,
    input: Input<M>,
    output: Output<N>,
    logic_builder: Option<B>,
}

impl<M: Kvt, B: LogicBuilder<M, N>, N: Kvt> OperatorBuilder<M, B, N> {
    /// start constructing by defining the operator name.
    /// with the new unlinked input and output (broadcasted).
    pub fn new(name: String) -> Self {
        OperatorBuilder {
            name,
            input: Input::new_unlinked(),
            output: Output::new_unlinked(full_broadcast),
            logic_builder: None,
        }
    }

    /// define the input
    pub fn with_input(mut self, input: Input<M>) -> Self {
        self.input = input;
        self
    }

    /// define the output
    pub fn with_output(mut self, output: Output<N>) -> Self {
        self.output = output;
        self
    }

    /// build the op
    pub fn build(self) -> Operator<M, B, N> {
        Operator::new(
            self.name,
            self.input,
            self.logic_builder.expect("logic_builder to be defined"),
            self.output,
        )
    }
}

impl<M, L, N> OperatorBuilder<M, DirectLogic<L>, N>
where
    M: Kvt,
    N: Kvt,
    L: Logic<M, N>,
{
    /// supply a logic directly
    pub fn with_direct_logic(mut self, logic: L) -> Self {
        self.logic_builder = Some(DirectLogic::new(logic));
        self
    }
}

#[cfg(test)]
mod tests {

    use std::{assert_matches, rc::Rc};

    use indexmap::IndexSet;
    use tokio::runtime::LocalRuntime;

    use crate::{
        channels::operator_io::{Input, Output, full_broadcast, link},
        runtime::{RuntimeFlavor, SingleThreadRuntimeFlavor},
        snapshot::{NoPersistence, PersistenceClient},
        stream::{SafeLogic, WorkerBuildContext, forward_logic::Forward},
        types::{DataMessage, OnceTime, Timestamp},
    };
    use malstrom_testkit::test_support::init_logs;

    use super::OperatorBuilder;
    #[test]
    fn builder_works_resuing_tests_common_primitives() {
        init_logs();
        type Kvt = (u8, u8, OnceTime);
    }

    /// check operator's wiring with the Builder
    /// TODO:
    ///   - reuse test primitives from other tests
    ///   - decide if this is an integration test
    ///   - probably come up with a unit test for the builder
    ///   - fix diagnostics
    ///   - MAIN:
    ///     - rewrite union and split operators using this new Builder
    ///     - ideally, hide all Operator internals from public API
    ///
    #[test]
    fn builder_works() {
        init_logs();
        type Kvt = (u8, u8, OnceTime);
        let mut sender = Output::<Kvt>::new_unlinked(full_broadcast);
        let mut input = Input::<Kvt>::new_unlinked();
        link(&mut sender, &mut input);

        let noop_logic = Forward::<Kvt>::new().into_logic();
        let mut forward = OperatorBuilder::new("forward-op".to_owned())
            .with_direct_logic(noop_logic)
            .with_input(input)
            .build();

        let mut receiver = Input::<Kvt>::new_unlinked();

        forward.link_to_input(&mut receiver);
        let persistence = NoPersistence {};
        let mut flavor = SingleThreadRuntimeFlavor::default();
        let operator_rt = Rc::new(LocalRuntime::new().unwrap());

        let worker_ctx = WorkerBuildContext::new(
            0,
            Rc::new(persistence) as Rc<dyn PersistenceClient>,
            Rc::new(flavor.communication().unwrap()),
            /*worker_ids*/ IndexSet::new(),
            /*config_version*/ 1,
            operator_rt.clone(),
        );

        operator_rt.clone().block_on(async move {
            let op_task = operator_rt.spawn_local(forward.start(worker_ctx.clone()));
            sender
                .send(crate::types::Message::Epoch(OnceTime::MIN))
                .await;
            assert_matches!(receiver.recv().await, crate::types::Message::Epoch(_));
            for i in 0..10 {
                let data = DataMessage::new(0u8, i as u8, OnceTime::MIN);
                sender.send(crate::types::Message::Data(data)).await;
                let x = receiver.recv().await;
                assert_matches!(x, crate::types::Message::Data(_data));
            }
            let data = DataMessage::new(0u8, 10, OnceTime::MAX);
            sender.send(crate::types::Message::Data(data)).await;
            let x = receiver.recv().await;
            assert_matches!(x, crate::types::Message::Data(_data));
            sender
                .send(crate::types::Message::Epoch(OnceTime::MAX))
                .await;
            assert_matches!(receiver.recv().await, crate::types::Message::Epoch(_));
            op_task.await.expect("operator to be finished");
        });
    }
}

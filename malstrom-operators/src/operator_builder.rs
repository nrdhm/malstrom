//! A builder to build JetStream operators

use malstrom_core::{
    channels::operator_io::{Input, Output, full_broadcast},
    stream::{DirectLogic, Logic, LogicBuilder, Operator},
    types::Kvt,
};

/// A convenient way to define an operator.
///
/// Crate-internal plumbing for the union/split combinators; not part of the user API.
pub(crate) struct OperatorBuilder<M: Kvt, B, N: Kvt> {
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

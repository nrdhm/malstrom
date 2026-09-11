//! Builder for datastreams

use std::marker::PhantomData;
use std::{rc::Rc, sync::Mutex};

use crate::channels::operator_io::{Input, Output, link};
use crate::stream::{OperatorContext, SafeLogic};
use crate::types::{DataMessage, Message};
use crate::{
    stream::{LogicBuilder, Operator},
    types::{Kvt, Sealed},
    worker::InnerRuntimeBuilder,
};

/// The StreamBuilder allows building datastreams
/// by calling operator methods like `.map` or `.filter` on it.
/// The StreamBuilder needs to be finished by dropping it, which will automatically
/// add it to the worker's execution schedule.
pub struct StreamBuilder<M: Kvt> {
    /// An Input that is already linked to the previous operator's Output.
    /// The receive-side of the last edge in the chain.
    pub tail: Input<M>,
    /// The runtime this stream is registered to.
    pub runtime: Rc<Mutex<InnerRuntimeBuilder>>,
}

impl<M> StreamBuilder<M>
where
    M: Kvt,
{
    /// Get a reference to the runtime this stream belongs to
    pub fn get_runtime(&self) -> Rc<Mutex<InnerRuntimeBuilder>> {
        Rc::clone(&self.runtime)
    }

    /// Recreate with the stream tail updated.
    pub fn with_new_tail(&self, new_tail: Input<M>) -> Self {
        StreamBuilder {
            tail: new_tail,
            runtime: Rc::clone(&self.runtime),
        }
    }

    /// Add a new no-op which will forward to the given Input.
    pub fn forward_tail_to(&mut self, name: String, new_input: &mut Input<M>) {
        let mut forward_op = Operator::direct(name, Forward(PhantomData::<M>).into_logic());
        // the forward operator gets the streams tail input, i.e. the input which receives from the
        // last operator in the given stream
        forward_op.swap_input(&mut self.tail);
        // link our forward output to the unioned input
        forward_op.link_to(new_input);
        // can't move out -- method received only a reference, not a value.
        // self.tail = *new_input;
        // can swap, but shouldn't -- others might not expect that.
        // std::mem::swap(&mut self.tail, new_input);
        self.runtime.lock().unwrap().add_operator(forward_op);
    }
}

struct Forward<Msg>(PhantomData<Msg>);
impl<Msg> SafeLogic<Msg, Msg> for Forward<Msg>
where
    Msg: Kvt,
{
    async fn on_data(
        &mut self,
        data_message: DataMessage<Msg>,
        output: &mut Output<Msg>,
        _ctx: &mut OperatorContext,
    ) {
        output.send(Message::Data(data_message)).await;
    }
}

/// The Builder of Datastreams.
pub trait Malstrom<M: Kvt>: Sealed {
    /// Connect the new operator's Input with the previous operator's Output.
    fn then<Out: Kvt, B: LogicBuilder<M, Out>>(
        self,
        operator: Operator<M, B, Out>,
    ) -> StreamBuilder<Out>;
}

/// Special-case builder to start a datastream.
pub struct InitialStreamBuilder {
    tail: Input<()>,
    // the runtime this stream is registered to
    runtime: Rc<Mutex<InnerRuntimeBuilder>>,
}
impl InitialStreamBuilder {
    pub(crate) fn new(input: Input<()>, runtime: Rc<Mutex<InnerRuntimeBuilder>>) -> Self {
        Self {
            tail: input,
            runtime,
        }
    }
}

impl Malstrom<()> for InitialStreamBuilder {
    fn then<Out: Kvt, B: LogicBuilder<(), Out>>(
        mut self,
        mut operator: Operator<(), B, Out>,
    ) -> StreamBuilder<Out> {
        std::mem::swap(&mut self.tail, operator.get_input_mut());
        let mut new_tail = Input::new_unlinked();
        link(operator.get_output_mut(), &mut new_tail);
        self.runtime.lock().unwrap().add_operator(operator);
        StreamBuilder {
            tail: new_tail,
            runtime: self.runtime,
        }
    }
}

impl<M> Malstrom<M> for StreamBuilder<M>
where
    M: Kvt,
{
    /// add an operator to the end of this stream
    /// and return a new stream where the new operator is last_op
    fn then<Out: Kvt, B: LogicBuilder<M, Out>>(
        mut self,
        mut operator: Operator<M, B, Out>,
    ) -> StreamBuilder<Out> {
        std::mem::swap(&mut self.tail, operator.get_input_mut());
        let mut new_tail = Input::new_unlinked();
        link(operator.get_output_mut(), &mut new_tail);
        self.runtime.lock().unwrap().add_operator(operator);
        StreamBuilder {
            tail: new_tail,
            runtime: self.runtime,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::channels::operator_io::{Input, Output, full_broadcast, link};
    use crate::types::{DataMessage, Message};

    type M = (u64, u64, u64);

    /// `link` wires an output to an input: a message sent on the output arrives on the
    /// linked input. `Malstrom::then` chains operators by exactly this primitive.
    #[tokio::test]
    async fn link_wires_output_to_input() {
        let mut output: Output<M> = Output::new_unlinked(full_broadcast);
        let mut input = Input::new_unlinked();
        link(&mut output, &mut input);

        output
            .send(Message::Data(DataMessage::new(1u64, 2u64, 3u64)))
            .await;
        let msg = input.recv().await;
        assert!(matches!(msg, Message::Data(d) if d.key == 1 && d.value == 2));
    }

    /// Linking twice yields two receivers; `Input::recv` drains them in order.
    #[tokio::test]
    async fn link_two_outputs_into_one_input() {
        let mut out_a: Output<M> = Output::new_unlinked(full_broadcast);
        let mut out_b: Output<M> = Output::new_unlinked(full_broadcast);
        let mut input = Input::new_unlinked();
        link(&mut out_a, &mut input);
        link(&mut out_b, &mut input);

        out_a
            .send(Message::Data(DataMessage::new(1u64, 1, 1)))
            .await;
        out_b
            .send(Message::Data(DataMessage::new(2u64, 2, 2)))
            .await;

        let mut seen = Vec::new();
        for _ in 0..2 {
            if let Message::Data(d) = input.recv().await {
                seen.push(d.key);
            }
        }
        seen.sort();
        assert_eq!(seen, vec![1, 2]);
    }
}

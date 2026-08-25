use std::marker::PhantomData;

use malstrom_core::channels::operator_io::{Input, Output};
use crate::operators::StreamSink;
use malstrom_core::stream::{Logic, Malstrom as _, Operator, OperatorContext, StreamBuilder};
use malstrom_core::types::{
        Data, DataMessage, Kvt, MaybeKey, MaybeTime, Message, NoData, NoKey, NoTime, Timestamp,
    };


/// A sink emitting records not hold any state (or only ephemeral state)
pub struct StatelessSink<In: Kvt, SinkImpl: StatelessSinkImpl<In>> {
    sink_impl: SinkImpl,
    _in_type: PhantomData<In>,
}

impl<In, SinkImpl> StatelessSink<In, SinkImpl>
where
    SinkImpl: StatelessSinkImpl<In>,
    In: Kvt,
{
    /// Create a new stateless sink by wrapping a sink implementation
    pub fn new(sink: SinkImpl) -> Self {
        Self {
            sink_impl: sink,
            _in_type: PhantomData,
        }
    }
}

/// Implementation of a stateless stream sink
pub trait StatelessSinkImpl<M: Kvt>: 'static {
    /// Emit a single record
    fn sink(&mut self, msg: DataMessage<M>);
}

impl<M, S> StreamSink<M> for StatelessSink<M, S>
where
    M: Kvt,
    S: StatelessSinkImpl<M>,
{
    fn consume_stream(self, name: &str, builder: StreamBuilder<M>) {
        builder.then(Operator::direct(name.into(), self));
    }
}

impl<M, S> Logic<M, ()> for StatelessSink<M, S>
where
    M: Kvt,
    S: StatelessSinkImpl<M>,
{
    async fn apply(
        &mut self,
        input: &mut Input<M>,
        output: &mut Output<()>,
        ctx: &mut OperatorContext,
    ) {
        match input.recv().await {
            Message::Data(d) => self.sink_impl.sink(d),
            // the sink has no downstream; the MAX epoch marks the end of the stream,
            // so close the output to signal completion
            Message::Epoch(e) if <M as Kvt>::Timestamp::CHECK_FINISHED(&Some(e.clone())) => {
                output.close()
            }
            _ => (),
        }
    }
}

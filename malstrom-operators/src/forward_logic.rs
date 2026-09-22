use std::marker::PhantomData;

use malstrom_core::{
    channels::operator_io::Output,
    stream::{OperatorContext, SafeLogic},
    types::{DataMessage, Kvt, Message},
};

/// No-op forwarding logic used to wire stream edges (union/split combinators).
///
/// Crate-internal: these combinators live in `malstrom-operators`, not the public API.
pub(crate) struct Forward<Msg>(PhantomData<Msg>);
impl<Msg> Forward<Msg>
where
    Msg: Kvt,
{
    pub fn new() -> Self {
        Forward(PhantomData::<Msg>)
    }
}

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

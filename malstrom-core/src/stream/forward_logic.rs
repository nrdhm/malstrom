use std::marker::PhantomData;

use crate::{
    channels::operator_io::Output,
    stream::{OperatorContext, SafeLogic},
    types::{DataMessage, Kvt, Message},
};

/// No-op forwarding logic used to wire stream edges (union/split combinators).
///
/// Implementation detail: not part of the user-facing extension API. Kept `pub` for
/// `malstrom-combinators`, hidden from docs. See the public-API surface audit
/// (`docs/overviews/08-public-api-surface.md`).
#[doc(hidden)]
pub struct Forward<Msg>(PhantomData<Msg>);
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

use std::marker::PhantomData;

use crate::{
    channels::operator_io::Output,
    stream::{OperatorContext, SafeLogic},
    types::{DataMessage, Kvt, Message},
};

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

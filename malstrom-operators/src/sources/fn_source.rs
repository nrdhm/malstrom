//! Convenience source constructors and the source implementations behind them.
//!
//! All of these are stateless (`PartitionState = ()`); stateful sources implement
//! [`SourceImpl`] directly and are wrapped with [`Source::from_impl`].

use std::{cell::RefCell, future::Future, marker::PhantomData, rc::Rc};

use futures::{Stream, StreamExt};

use crate::sources::{Source, SourceImpl, SourcePartition};
use malstrom::types::{Data, Key, NoKey, OnceTime, Timestamp, distributable::Distributable};


/// An untimed source reading from an iterator.
///
/// Every record is timestamped with [`OnceTime(false)`]; the stream finishes with
/// `OnceTime(true)`.
pub struct FromIteratorSource<V> {
    iter: Option<Box<dyn Iterator<Item = V>>>,
}

impl<V> FromIteratorSource<V> {
    pub(crate) fn new(iter: impl IntoIterator<Item = V> + 'static) -> Self {
        Self {
            iter: Some(Box::new(iter.into_iter())),
        }
    }
}

impl<V> SourceImpl for FromIteratorSource<V>
where
    V: Distributable + Data,
{
    type PartitionKey = NoKey;
    type Value = V;
    type Timestamp = OnceTime;
    type PartitionState = ();
    type Partition = FromIteratorPartition<V>;

    async fn discover(&mut self) -> Vec<Self::PartitionKey> {
        vec![NoKey]
    }

    async fn open(&mut self, _key: &NoKey, _state: Option<()>) -> Self::Partition {
        FromIteratorPartition {
            iter: Some(
                self.iter
                    .take()
                    .expect("FromIteratorSource has exactly one partition"),
            ),
        }
    }
}

/// The single partition of a [FromIteratorSource].
pub struct FromIteratorPartition<V> {
    iter: Option<Box<dyn Iterator<Item = V>>>,
}

impl<V> SourcePartition for FromIteratorPartition<V>
where
    V: Distributable + Data,
{
    type PartitionKey = NoKey;
    type Value = V;
    type Timestamp = OnceTime;
    type State = ();

    async fn poll(&mut self) -> Option<(V, OnceTime)> {
        self.iter
            .as_mut()
            .and_then(|it| it.next())
            .map(|v| (v, OnceTime::MIN))
    }

    async fn snapshot(&self) {}

    async fn collect(self) {}
}

/// A source reading from an iterator, timestamping each record with its index.
pub struct FromEnumeratedIteratorSource<V> {
    iter: Option<Box<dyn Iterator<Item = V>>>,
}

impl<V> FromEnumeratedIteratorSource<V> {
    pub(crate) fn new(iter: impl IntoIterator<Item = V> + 'static) -> Self {
        Self {
            iter: Some(Box::new(iter.into_iter())),
        }
    }
}

impl<V> SourceImpl for FromEnumeratedIteratorSource<V>
where
    V: Distributable + Data,
{
    type PartitionKey = NoKey;
    type Value = V;
    type Timestamp = usize;
    type PartitionState = ();
    type Partition = FromEnumeratedIteratorPartition<V>;

    async fn discover(&mut self) -> Vec<Self::PartitionKey> {
        vec![NoKey]
    }

    async fn open(&mut self, _key: &NoKey, _state: Option<()>) -> Self::Partition {
        FromEnumeratedIteratorPartition {
            iter: Some(
                self.iter
                    .take()
                    .expect("FromEnumeratedIteratorSource has exactly one partition"),
            ),
            next: 0,
        }
    }
}

/// The single partition of a [FromEnumeratedIteratorSource].
pub struct FromEnumeratedIteratorPartition<V> {
    iter: Option<Box<dyn Iterator<Item = V>>>,
    next: usize,
}

impl<V> SourcePartition for FromEnumeratedIteratorPartition<V>
where
    V: Distributable + Data,
{
    type PartitionKey = NoKey;
    type Value = V;
    type Timestamp = usize;
    type State = ();

    async fn poll(&mut self) -> Option<(V, usize)> {
        let item = self.iter.as_mut().and_then(|it| it.next())?;
        let ts = self.next;
        self.next += 1;
        Some((item, ts))
    }

    async fn snapshot(&self) {}

    async fn collect(self) {}
}

/// A source built from a poll closure returning `Option<(Value, Timestamp)>`.
pub struct PollSource<V, T, F> {
    f: Option<F>,
    _m: PhantomData<(V, T)>,
}

impl<V, T, F> PollSource<V, T, F> {
    pub(crate) fn new(f: F) -> Self {
        Self {
            f: Some(f),
            _m: PhantomData,
        }
    }
}

impl<V, T, Fut, F> SourceImpl for PollSource<V, T, F>
where
    V: Distributable + Data,
    T: Distributable + Timestamp,
    Fut: Future<Output = Option<(V, T)>>,
    F: FnMut() -> Fut + 'static,
{
    type PartitionKey = NoKey;
    type Value = V;
    type Timestamp = T;
    type PartitionState = ();
    type Partition = PollPartition<V, T, F>;

    async fn discover(&mut self) -> Vec<Self::PartitionKey> {
        vec![NoKey]
    }

    async fn open(&mut self, _key: &NoKey, _state: Option<()>) -> Self::Partition {
        PollPartition {
            f: Rc::new(RefCell::new(
                self.f.take().expect("PollSource has exactly one partition"),
            )),
            _m: PhantomData,
        }
    }
}

/// The single partition of a [PollSource].
pub struct PollPartition<V, T, F> {
    f: Rc<RefCell<F>>,
    _m: PhantomData<(V, T)>,
}

impl<V, T, Fut, F> SourcePartition for PollPartition<V, T, F>
where
    V: Distributable + Data,
    T: Distributable + Timestamp,
    Fut: Future<Output = Option<(V, T)>>,
    F: FnMut() -> Fut + 'static,
{
    type PartitionKey = NoKey;
    type Value = V;
    type Timestamp = T;
    type State = ();

    async fn poll(&mut self) -> Option<(V, T)> {
        (self.f.borrow_mut())().await
    }

    async fn snapshot(&self) {}

    async fn collect(self) {}
}

/// A source reading from a [`Stream`] of `(value, timestamp)` pairs.
pub struct FromStreamSource<V, T, S> {
    stream: Option<S>,
    _m: PhantomData<(V, T)>,
}

impl<V, T, S> FromStreamSource<V, T, S> {
    pub(crate) fn new(stream: S) -> Self {
        Self {
            stream: Some(stream),
            _m: PhantomData,
        }
    }
}

impl<V, T, S> SourceImpl for FromStreamSource<V, T, S>
where
    V: Distributable + Data,
    T: Distributable + Timestamp,
    S: Stream<Item = (V, T)> + Unpin + 'static,
{
    type PartitionKey = NoKey;
    type Value = V;
    type Timestamp = T;
    type PartitionState = ();
    type Partition = FromStreamPartition<V, T, S>;

    async fn discover(&mut self) -> Vec<Self::PartitionKey> {
        vec![NoKey]
    }

    async fn open(&mut self, _key: &NoKey, _state: Option<()>) -> Self::Partition {
        FromStreamPartition {
            stream: self
                .stream
                .take()
                .expect("FromStreamSource has exactly one partition"),
            _m: PhantomData,
        }
    }
}

/// The single partition of a [FromStreamSource].
pub struct FromStreamPartition<V, T, S> {
    stream: S,
    _m: PhantomData<(V, T)>,
}

impl<V, T, S> SourcePartition for FromStreamPartition<V, T, S>
where
    V: Distributable + Data,
    T: Distributable + Timestamp,
    S: Stream<Item = (V, T)> + Unpin + 'static,
{
    type PartitionKey = NoKey;
    type Value = V;
    type Timestamp = T;
    type State = ();

    async fn poll(&mut self) -> Option<(V, T)> {
        self.stream.next().await
    }

    async fn snapshot(&self) {}

    async fn collect(self) {}
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use malstrom::channels::operator_io::{Input, Output};
use crate::operators::{Sink, Source as _};
use crate::sinks::{StatelessSink, VecSink};
use crate::sources::Source;
use malstrom::stream::{Malstrom as _, Operator, OperatorContext, StreamBuilder};
use malstrom_testkit::get_test_rt;
use malstrom::types::{Message, NoKey};


    /// The from_iterator source should emit the iterator values, untimed
    #[test]
    fn from_iterator_emits_values() {
        let in_data: Vec<i32> = (0..100).collect();
        let collector = VecSink::new();
        let rt = get_test_rt(|provider| {
            let in_data = in_data.clone();
            provider
                .new_stream()
                .source("source", Source::from_iterator(in_data))
                .sink("sink", StatelessSink::new(collector.clone()));
        });
        rt.execute().unwrap();

        let c = collector.into_iter().map(|x| x.value).collect_vec();
        assert_eq!(c, (0..100).collect_vec())
    }

    /// from_enumerated_iterator timestamps values with their iterator index
    #[test]
    fn from_enumerated_iterator_emits_timestamped_messages() {
        let sink = VecSink::new();
        let rt = get_test_rt(|provider| {
            provider
                .new_stream()
                .source("source", Source::from_enumerated_iterator(42..52))
                .sink("sink", StatelessSink::new(sink.clone()));
        });
        rt.execute().unwrap();

        let timestamps = sink.into_iter().map(|x| x.timestamp).collect_vec();
        let expected = (0..10).collect_vec();
        assert_eq!(expected, timestamps);
    }

    /// after the final value a single MAX epoch should be emitted
    #[test]
    fn from_enumerated_iterator_emits_max_epoch() {
        type Msg = (NoKey, i32, usize);
        let sink = VecSink::new();
        let rt = get_test_rt(|provider| {
            let sink = sink.clone();
            provider
                .new_stream()
                .source("source", Source::from_enumerated_iterator(0..10))
                .then(Operator::direct(
                    "sink-epochs".to_string(),
                    async move |input: &mut Input<Msg>,
                                output: &mut Output<Msg>,
                                _ctx: &mut OperatorContext| {
                        let msg = input.recv().await;
                        match msg {
                            Message::Epoch(x) => {
                                println!("epoch: {x:?}");
                                sink.give(x.clone());
                                output.send(Message::Epoch(x)).await;
                            }
                            msg => {
                                // eprintln!("{msg:?}");
                                output.send(msg).await;
                            }
                        }
                    },
                ));
        });
        rt.execute().unwrap();

        let messages = sink.drain_vec(..);
        let last = messages.last().unwrap();
        assert_eq!(*last, usize::MAX);
    }
}

use malstrom_core::stream::{Malstrom, StreamBuilder};
use malstrom_core::types::{Data, Kvt, MaybeKey, Sealed, Timestamp};

/// Output messages from a Malstrom stream somewhere
pub trait Sink<M, S>: Sealed {
    /// Sink all messages in this stream to the given output.
    /// This will consume the messages. If you whish to write to multiple outputs,
    /// consider calling [.cloned()](crate::operators::Cloned::cloned) on the stream.
    ///
    /// # Example
    ///
    /// ```
    /// use malstrom_operators::operators::*;
    /// use malstrom_operators::operators::Source as _;
    /// use malstrom_core::runtime::SingleThreadRuntime;
    /// use malstrom_core::snapshot::NoPersistence;
    /// use malstrom_operators::sources::Source;
    /// use malstrom_core::worker::StreamProvider;
    /// use malstrom_operators::sinks::{VecSink, StatelessSink};
    ///
    /// let sink = VecSink::new();
    /// let sink_clone = sink.clone();
    ///
    /// SingleThreadRuntime::builder()
    ///     .persistence(NoPersistence)
    ///     .build(move |provider: &mut dyn StreamProvider| {
    ///         provider.new_stream()
    ///         .source("numbers", Source::from_iterator(0..10))
    ///         .sink("sink", StatelessSink::new(sink_clone));
    ///     })
    ///     .execute()
    ///     .unwrap();
    /// let expected: Vec<i32> = (0..10).collect();
    /// let out: Vec<i32> = sink.into_iter().map(|x| x.value).collect();
    /// assert_eq!(out, expected);
    /// ```
    fn sink(self, name: &str, sink: S);
}

/// A stream output which takes messages, usually producing them to some external system.
/// For users it is normally not necessary to implement this trait unless they are writing
/// custom outputs for sinks which Malstrom does not (yet) support.
#[diagnostic::on_unimplemented(message = "Not a Sink: 
    You might need to wrap this in `StatefulSink::new` or `StatelessSink::new`")]
pub trait StreamSink<M: Kvt> {
    /// Consume a datastream to the end.
    fn consume_stream(self, name: &str, builder: StreamBuilder<M>);
}

impl<M, S> Sink<M, S> for StreamBuilder<M>
where
    M: Kvt,
    S: StreamSink<M>,
{
    fn sink(self, name: &str, sink: S) {
        sink.consume_stream(name, self)
    }
}
